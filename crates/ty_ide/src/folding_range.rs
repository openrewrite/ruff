use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::source_text;
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{AnyNodeRef, Stmt};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::Db;

/// The kind of a folding range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldingRangeKind {
    /// A comment block.
    Comment,
    /// An import block.
    Imports,
    /// A region (e.g., `# region` / `# endregion`).
    Region,
}

/// A folding range in the source code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldingRange {
    /// The range to fold.
    pub range: TextRange,
    /// The kind of folding range.
    pub kind: Option<FoldingRangeKind>,
}

impl FoldingRange {
    fn with_kind(self, kind: FoldingRangeKind) -> Self {
        Self {
            kind: Some(kind),
            ..self
        }
    }
}

impl From<TextRange> for FoldingRange {
    fn from(range: TextRange) -> FoldingRange {
        FoldingRange { range, kind: None }
    }
}

/// Returns a list of folding ranges for the given file.
pub fn folding_ranges(db: &dyn Db, file: File) -> Vec<FoldingRange> {
    let parsed = parsed_module(db, file).load(db);
    let source = source_text(db, file);

    let mut visitor = FoldingRangeVisitor {
        source: source.as_str(),
        ranges: vec![],
    };
    visitor.visit_body(parsed.suite());

    // Add docstring for module-level (first statement if it's a string literal).
    visitor.add_docstring_range(parsed.suite());

    // Add remaining ranges not covered by the AST visitor.
    visitor.add_import_ranges(parsed.suite());
    visitor.add_comment_ranges();
    visitor.add_custom_region_ranges();

    visitor.ranges
}

struct FoldingRangeVisitor<'a> {
    source: &'a str,
    ranges: Vec<FoldingRange>,
}

impl<'a> FoldingRangeVisitor<'a> {
    /// Only add folding ranges that span multiple lines.
    fn add_range(&mut self, folding_range: impl Into<FoldingRange>) {
        let folding_range = folding_range.into();
        if !self.is_multiline(folding_range.range) {
            return;
        }
        self.ranges.push(folding_range);
    }

    /// Iterate over lines with their starting byte offsets.
    fn lines_with_indices(&self) -> impl Iterator<Item = (TextSize, &'a str)> + use<'a> {
        let mut offset = TextSize::new(0);
        self.source.lines().map(move |line| {
            let current_offset = offset;
            // +1 for the newline character (except for the last line potentially)
            offset += TextSize::of(line) + TextSize::new(1);
            (current_offset, line)
        })
    }

    fn is_multiline(&self, range: TextRange) -> bool {
        self.source[range].contains('\n')
    }

    /// Compute folding ranges for consecutive import statements.
    fn add_import_ranges(&mut self, stmts: &[Stmt]) {
        let mut import_range: Option<TextRange> = None;

        for stmt in stmts {
            if matches!(stmt, Stmt::Import(_) | Stmt::ImportFrom(_)) {
                if let Some(ref mut range) = import_range {
                    *range = range.with_end(stmt.end());
                } else {
                    import_range = Some(stmt.range());
                }
            } else if let Some(range) = import_range {
                self.add_range(FoldingRange::from(range).with_kind(FoldingRangeKind::Imports));
                import_range = None;
            }
        }
        if let Some(range) = import_range {
            self.add_range(FoldingRange::from(range).with_kind(FoldingRangeKind::Imports));
        }
    }

    /// Compute folding ranges for `# region` / `# endregion` comments.
    fn add_custom_region_ranges(&mut self) {
        let mut region_starts: Vec<TextSize> = Vec::new();

        for (offset, line) in self.lines_with_indices() {
            let trimmed = line.trim();
            if trimmed.starts_with("# region") || trimmed.starts_with("#region") {
                region_starts.push(offset);
            } else if trimmed.starts_with("# endregion") || trimmed.starts_with("#endregion") {
                if let Some(start) = region_starts.pop() {
                    let end = offset + TextSize::of(line.trim_end());
                    self.add_range(
                        FoldingRange::from(TextRange::new(start, end))
                            .with_kind(FoldingRangeKind::Region),
                    );
                }
            }
        }
    }

    /// Compute folding ranges for consecutive comment lines.
    fn add_comment_ranges(&mut self) {
        let mut comment_range: Option<TextRange> = None;

        for (line_start, line) in self.lines_with_indices() {
            let trimmed = line.trim();

            // Check if this is a comment line (but not a region marker)
            let is_comment = trimmed.starts_with('#')
                && !trimmed.starts_with("# region")
                && !trimmed.starts_with("#region")
                && !trimmed.starts_with("# endregion")
                && !trimmed.starts_with("#endregion");

            if is_comment {
                let end = line_start + TextSize::of(line.trim_end());
                if let Some(ref mut range) = comment_range {
                    *range = range.with_end(end);
                } else {
                    comment_range = Some(TextRange::new(line_start, end));
                }
            } else if let Some(range) = comment_range {
                self.add_range(FoldingRange::from(range).with_kind(FoldingRangeKind::Comment));
                comment_range = None;
            }
        }
        if let Some(range) = comment_range {
            self.add_range(FoldingRange::from(range).with_kind(FoldingRangeKind::Comment));
        }
    }

    /// Add a folding range for a docstring if present at the start of a body.
    fn add_docstring_range(&mut self, body: &[Stmt]) {
        let Some(first_stmt) = body.first() else {
            return;
        };
        let Stmt::Expr(ref expr_stmt) = *first_stmt else {
            return;
        };
        if !expr_stmt.value.is_string_literal_expr() {
            return;
        }
        self.add_range(FoldingRange::from(first_stmt.range()).with_kind(FoldingRangeKind::Comment));
    }
}

impl SourceOrderVisitor<'_> for FoldingRangeVisitor<'_> {
    fn enter_node(&mut self, node: AnyNodeRef<'_>) -> TraversalSignal {
        match node {
            // Compound statements that create folding regions
            AnyNodeRef::StmtFunctionDef(func) => {
                self.add_range(func.range());
                self.add_docstring_range(&func.body);
            }
            AnyNodeRef::StmtClassDef(class) => {
                self.add_range(class.range());
                self.add_docstring_range(&class.body);
            }
            AnyNodeRef::StmtIf(if_stmt) => {
                self.add_range(if_stmt.range());
            }
            AnyNodeRef::StmtFor(for_stmt) => {
                self.add_range(for_stmt.range());
            }
            AnyNodeRef::StmtWhile(while_stmt) => {
                self.add_range(while_stmt.range());
            }
            AnyNodeRef::StmtWith(with_stmt) => {
                self.add_range(with_stmt.range());
            }
            AnyNodeRef::StmtTry(try_stmt) => {
                self.add_range(try_stmt.range());
            }
            AnyNodeRef::StmtMatch(match_stmt) => {
                self.add_range(match_stmt.range());
            }

            // Match cases within match statements
            AnyNodeRef::MatchCase(case) => {
                self.add_range(case.range());
            }

            // Exception handlers
            AnyNodeRef::ExceptHandlerExceptHandler(handler) => {
                self.add_range(handler.range());
            }

            // Multiline expressions
            AnyNodeRef::ExprList(list) => {
                self.add_range(list.range());
            }
            AnyNodeRef::ExprTuple(tuple) => {
                // Only fold parenthesized tuples.
                if tuple.parenthesized {
                    self.add_range(tuple.range());
                }
            }
            AnyNodeRef::ExprDict(dict) => {
                self.add_range(dict.range());
            }
            AnyNodeRef::ExprSet(set) => {
                self.add_range(set.range());
            }
            AnyNodeRef::ExprListComp(listcomp) => {
                self.add_range(listcomp.range());
            }
            AnyNodeRef::ExprSetComp(setcomp) => {
                self.add_range(setcomp.range());
            }
            AnyNodeRef::ExprDictComp(dictcomp) => {
                self.add_range(dictcomp.range());
            }
            AnyNodeRef::ExprGenerator(generator) => {
                self.add_range(generator.range());
            }

            // Function calls with arguments spanning multiple lines
            AnyNodeRef::ExprCall(call) => {
                self.add_range(call.range());
            }

            // String literals
            AnyNodeRef::ExprStringLiteral(string) => {
                self.add_range(string.range());
            }
            AnyNodeRef::ExprFString(fstring) => {
                self.add_range(fstring.range());
            }

            // Type parameter lists
            AnyNodeRef::TypeParams(params) => {
                self.add_range(params.range());
            }

            _ => {}
        }

        TraversalSignal::Traverse
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::CursorTest;
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span};

    #[test]
    fn test_folding_range_class() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
class MyClass:
    def __init__(self):
        self.value = 1

    def method(self):
        return self.value
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range
         --> main.py:2:1
          |
        2 | / class MyClass:
        3 | |     def __init__(self):
        4 | |         self.value = 1
        5 | |
        6 | |     def method(self):
        7 | |         return self.value
          | |_________________________^
          |

        info[folding-range]: Folding Range
         --> main.py:3:5
          |
        2 |   class MyClass:
        3 | /     def __init__(self):
        4 | |         self.value = 1
          | |______________________^
        5 |
        6 |       def method(self):
          |

        info[folding-range]: Folding Range
         --> main.py:6:5
          |
        4 |           self.value = 1
        5 |
        6 | /     def method(self):
        7 | |         return self.value
          | |_________________________^
          |
        ");
    }

    #[test]
    fn test_folding_range_attribute_comments() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
class MyClass:
    def __init__(self):
        self.value = 1
        """
        This is an
        attribute comment.
        """
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r#"
        info[folding-range]: Folding Range
         --> main.py:2:1
          |
        2 | / class MyClass:
        3 | |     def __init__(self):
        4 | |         self.value = 1
        5 | |         """
        6 | |         This is an
        7 | |         attribute comment.
        8 | |         """
          | |___________^
          |

        info[folding-range]: Folding Range
         --> main.py:3:5
          |
        2 |   class MyClass:
        3 | /     def __init__(self):
        4 | |         self.value = 1
        5 | |         """
        6 | |         This is an
        7 | |         attribute comment.
        8 | |         """
          | |___________^
          |

        info[folding-range]: Folding Range
         --> main.py:5:9
          |
        3 |       def __init__(self):
        4 |           self.value = 1
        5 | /         """
        6 | |         This is an
        7 | |         attribute comment.
        8 | |         """
          | |___________^
          |
        "#);
    }

    #[test]
    fn test_folding_range_imports() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
import os
import sys
from typing import List, Dict
<CURSOR>
def main():
    pass
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range
         --> main.py:6:1
          |
        4 |   from typing import List, Dict
        5 |
        6 | / def main():
        7 | |     pass
          | |________^
          |

        info[folding-range]: Folding Range (imports)
         --> main.py:2:1
          |
        2 | / import os
        3 | | import sys
        4 | | from typing import List, Dict
          | |_____________________________^
        5 |
        6 |   def main():
          |
        ");
    }

    #[test]
    fn test_folding_range_control_flow() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
if condition:
    do_something()
elif other:
    do_other()
else:
    default()

for item in items:
    process(item)

while running:
    continue_work()
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range
         --> main.py:2:1
          |
        2 | / if condition:
        3 | |     do_something()
        4 | | elif other:
        5 | |     do_other()
        6 | | else:
        7 | |     default()
          | |_____________^
        8 |
        9 |   for item in items:
          |

        info[folding-range]: Folding Range
          --> main.py:9:1
           |
         7 |       default()
         8 |
         9 | / for item in items:
        10 | |     process(item)
           | |_________________^
        11 |
        12 |   while running:
           |

        info[folding-range]: Folding Range
          --> main.py:12:1
           |
        10 |       process(item)
        11 |
        12 | / while running:
        13 | |     continue_work()
           | |___________________^
           |
        ");
    }

    #[test]
    fn test_folding_range_try_except() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
try:
    risky_operation()
except ValueError:
    handle_value_error()
except TypeError:
    handle_type_error()
finally:
    cleanup()
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range
         --> main.py:2:1
          |
        2 | / try:
        3 | |     risky_operation()
        4 | | except ValueError:
        5 | |     handle_value_error()
        6 | | except TypeError:
        7 | |     handle_type_error()
        8 | | finally:
        9 | |     cleanup()
          | |_____________^
          |

        info[folding-range]: Folding Range
         --> main.py:4:1
          |
        2 |   try:
        3 |       risky_operation()
        4 | / except ValueError:
        5 | |     handle_value_error()
          | |________________________^
        6 |   except TypeError:
        7 |       handle_type_error()
          |

        info[folding-range]: Folding Range
         --> main.py:6:1
          |
        4 |   except ValueError:
        5 |       handle_value_error()
        6 | / except TypeError:
        7 | |     handle_type_error()
          | |_______________________^
        8 |   finally:
        9 |       cleanup()
          |
        ");
    }

    #[test]
    fn test_folding_range_collections() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
my_list = [
    1,
    2,
    3,
]

my_dict = {
    "a": 1,
    "b": 2,
}
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r#"
        info[folding-range]: Folding Range
         --> main.py:2:11
          |
        2 |   my_list = [
          |  ___________^
        3 | |     1,
        4 | |     2,
        5 | |     3,
        6 | | ]
          | |_^
        7 |
        8 |   my_dict = {
          |

        info[folding-range]: Folding Range
          --> main.py:8:11
           |
         6 |   ]
         7 |
         8 |   my_dict = {
           |  ___________^
         9 | |     "a": 1,
        10 | |     "b": 2,
        11 | | }
           | |_^
           |
        "#);
    }

    #[test]
    fn test_folding_range_match() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
match value:
    case 1:
        one()
    case 2:
        two()
    case _:
        default()
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range
         --> main.py:2:1
          |
        2 | / match value:
        3 | |     case 1:
        4 | |         one()
        5 | |     case 2:
        6 | |         two()
        7 | |     case _:
        8 | |         default()
          | |_________________^
          |

        info[folding-range]: Folding Range
         --> main.py:3:5
          |
        2 |   match value:
        3 | /     case 1:
        4 | |         one()
          | |_____________^
        5 |       case 2:
        6 |           two()
          |

        info[folding-range]: Folding Range
         --> main.py:5:5
          |
        3 |       case 1:
        4 |           one()
        5 | /     case 2:
        6 | |         two()
          | |_____________^
        7 |       case _:
        8 |           default()
          |

        info[folding-range]: Folding Range
         --> main.py:7:5
          |
        5 |       case 2:
        6 |           two()
        7 | /     case _:
        8 | |         default()
          | |_________________^
          |
        ");
    }

    #[test]
    fn test_folding_range_regions() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
# region Imports
import os
import sys
# endregion

# region Main
def main():
    pass
# endregion
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r"
        info[folding-range]: Folding Range
          --> main.py:8:1
           |
         7 |   # region Main
         8 | / def main():
         9 | |     pass
           | |________^
        10 |   # endregion
           |

        info[folding-range]: Folding Range (imports)
         --> main.py:3:1
          |
        2 |   # region Imports
        3 | / import os
        4 | | import sys
          | |__________^
        5 |   # endregion
          |

        info[folding-range]: Folding Range (region)
         --> main.py:2:1
          |
        2 | / # region Imports
        3 | | import os
        4 | | import sys
        5 | | # endregion
          | |___________^
        6 |
        7 |   # region Main
          |

        info[folding-range]: Folding Range (region)
          --> main.py:7:1
           |
         5 |   # endregion
         6 |
         7 | / # region Main
         8 | | def main():
         9 | |     pass
        10 | | # endregion
           | |___________^
           |
        ");
    }

    #[test]
    fn test_folding_range_docstring() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
def my_function():
    """
    This is a multiline
    docstring.
    """
    pass
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r#"
        info[folding-range]: Folding Range
         --> main.py:2:1
          |
        2 | / def my_function():
        3 | |     """
        4 | |     This is a multiline
        5 | |     docstring.
        6 | |     """
        7 | |     pass
          | |________^
          |

        info[folding-range]: Folding Range (comment)
         --> main.py:3:5
          |
        2 |   def my_function():
        3 | /     """
        4 | |     This is a multiline
        5 | |     docstring.
        6 | |     """
          | |_______^
        7 |       pass
          |

        info[folding-range]: Folding Range
         --> main.py:3:5
          |
        2 |   def my_function():
        3 | /     """
        4 | |     This is a multiline
        5 | |     docstring.
        6 | |     """
          | |_______^
        7 |       pass
          |
        "#);
    }

    #[test]
    fn test_folding_range_comments() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
# This is a comment block
# that spans multiple lines
# explaining something important

def foo():
    pass

# Another comment block
# with more details
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(
            test.folding_ranges(),
            @r"
        info[folding-range]: Folding Range
         --> main.py:6:1
          |
        4 |   # explaining something important
        5 |
        6 | / def foo():
        7 | |     pass
          | |________^
        8 |
        9 |   # Another comment block
          |

        info[folding-range]: Folding Range (comment)
         --> main.py:2:1
          |
        2 | / # This is a comment block
        3 | | # that spans multiple lines
        4 | | # explaining something important
          | |________________________________^
        5 |
        6 |   def foo():
          |

        info[folding-range]: Folding Range (comment)
          --> main.py:9:1
           |
         7 |       pass
         8 |
         9 | / # Another comment block
        10 | | # with more details
           | |___________________^
           |
        ",
        );
    }

    #[test]
    fn test_folding_range_with() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
with open("file.txt") as f:
    content = f.read()
    process(content)
<CURSOR>
"#,
            )
            .build();

        assert_snapshot!(test.folding_ranges(), @r#"
        info[folding-range]: Folding Range
         --> main.py:2:1
          |
        2 | / with open("file.txt") as f:
        3 | |     content = f.read()
        4 | |     process(content)
          | |____________________^
          |
        "#);
    }

    impl CursorTest {
        fn folding_ranges(&self) -> String {
            let ranges = folding_ranges(&self.db, self.cursor.file);

            if ranges.is_empty() {
                return "No folding ranges found".to_string();
            }

            let diagnostics: Vec<FoldingRangeDiagnostic> = ranges
                .into_iter()
                .map(|fr| FoldingRangeDiagnostic::new(self.cursor.file, fr))
                .collect();

            self.render_diagnostics(diagnostics)
        }
    }

    struct FoldingRangeDiagnostic {
        file: File,
        folding_range: FoldingRange,
    }

    impl FoldingRangeDiagnostic {
        fn new(file: File, folding_range: FoldingRange) -> Self {
            Self {
                file,
                folding_range,
            }
        }
    }

    impl crate::tests::IntoDiagnostic for FoldingRangeDiagnostic {
        fn into_diagnostic(self) -> Diagnostic {
            let message = match self.folding_range.kind {
                Some(FoldingRangeKind::Comment) => "Folding Range (comment)",
                Some(FoldingRangeKind::Imports) => "Folding Range (imports)",
                Some(FoldingRangeKind::Region) => "Folding Range (region)",
                None => "Folding Range",
            };

            let mut diagnostic = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("folding-range")),
                Severity::Info,
                message.to_string(),
            );

            diagnostic.annotate(Annotation::primary(
                Span::from(self.file).with_range(self.folding_range.range),
            ));

            diagnostic
        }
    }
}
