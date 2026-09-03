//! A home for deferred checks that must be done after the `TypeInferenceBuilder` has done an initial
//! inference pass over the whole scope.

pub mod decorator;
pub mod dynamic_class;
pub mod final_variable;
pub mod function;
pub mod overloaded_function;
pub mod pep_613_alias;
pub mod static_class;
pub mod type_param_validation;
pub mod typed_dict;
pub mod typeguard;
