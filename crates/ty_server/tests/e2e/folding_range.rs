use anyhow::Result;
use ruff_db::system::SystemPath;

use crate::TestServerBuilder;

#[test]
fn folding_range_class_and_functions() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = r#"class MyClass:
    def __init__(self):
        self.value = 1

    def method(self):
        return self.value
"#;

    let mut server = TestServerBuilder::new()?
        .enable_pull_diagnostics(true)
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let ranges = server.folding_range_request(&server.file_uri(foo));

    insta::assert_json_snapshot!(ranges);

    Ok(())
}

#[test]
fn folding_range_imports() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = r#"import os
import sys
from typing import List, Dict

def main():
    pass
"#;

    let mut server = TestServerBuilder::new()?
        .enable_pull_diagnostics(true)
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let ranges = server.folding_range_request(&server.file_uri(foo));

    insta::assert_json_snapshot!(ranges);

    Ok(())
}

#[test]
fn folding_range_control_flow() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = r#"if condition:
    do_something()
elif other:
    do_other()
else:
    default()

for item in items:
    process(item)

while running:
    continue_work()
"#;

    let mut server = TestServerBuilder::new()?
        .enable_pull_diagnostics(true)
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let ranges = server.folding_range_request(&server.file_uri(foo));

    insta::assert_json_snapshot!(ranges);

    Ok(())
}

#[test]
fn folding_range_try_except() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = r#"try:
    risky_operation()
except ValueError:
    handle_value_error()
except TypeError:
    handle_type_error()
finally:
    cleanup()
"#;

    let mut server = TestServerBuilder::new()?
        .enable_pull_diagnostics(true)
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let ranges = server.folding_range_request(&server.file_uri(foo));

    insta::assert_json_snapshot!(ranges);

    Ok(())
}

#[test]
fn folding_range_multiline_collections() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = r#"my_list = [
    1,
    2,
    3,
]

my_dict = {
    "a": 1,
    "b": 2,
}
"#;

    let mut server = TestServerBuilder::new()?
        .enable_pull_diagnostics(true)
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let ranges = server.folding_range_request(&server.file_uri(foo));

    insta::assert_json_snapshot!(ranges);

    Ok(())
}

#[test]
fn folding_range_regions() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = r#"# region Imports
import os
import sys
# endregion

# region Main
def main():
    pass
# endregion
"#;

    let mut server = TestServerBuilder::new()?
        .enable_pull_diagnostics(true)
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let ranges = server.folding_range_request(&server.file_uri(foo));

    insta::assert_json_snapshot!(ranges);

    Ok(())
}

#[test]
fn folding_range_docstring() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = r#"def my_function():
    """
    This is a multiline
    docstring.
    """
    pass
"#;

    let mut server = TestServerBuilder::new()?
        .enable_pull_diagnostics(true)
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let ranges = server.folding_range_request(&server.file_uri(foo));

    insta::assert_json_snapshot!(ranges);

    Ok(())
}

#[test]
fn folding_range_match_statement() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = r#"match value:
    case 1:
        one()
    case 2:
        two()
    case _:
        default()
"#;

    let mut server = TestServerBuilder::new()?
        .enable_pull_diagnostics(true)
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let ranges = server.folding_range_request(&server.file_uri(foo));

    insta::assert_json_snapshot!(ranges);

    Ok(())
}

#[test]
fn folding_range_comments() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = r#"# This is a comment block
# that spans multiple lines
# explaining something important

def foo():
    pass

# Another comment block
# with more details
"#;

    let mut server = TestServerBuilder::new()?
        .enable_pull_diagnostics(true)
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let ranges = server.folding_range_request(&server.file_uri(foo));

    insta::assert_json_snapshot!(ranges);

    Ok(())
}
