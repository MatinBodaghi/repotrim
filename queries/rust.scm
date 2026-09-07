;; ====================================================================
;; Rust Tree-sitter Queries for Repotrim Symbol and Reference Extraction
;; ====================================================================

;; Function items (capturing name, parameters, return_type, and optional body)
(function_item
  name: (identifier) @function.name
  parameters: (parameters) @function.parameters
  return_type: (_)? @function.return_type
  body: (block)? @function.body) @function

;; Trait function signature items (e.g. fn foo(&self);)
(function_signature_item
  name: (identifier) @function.name
  parameters: (parameters) @function.parameters
  return_type: (_)? @function.return_type) @function

;; Struct items (capturing name)
(struct_item
  name: (type_identifier) @struct.name) @struct

;; Enum items (capturing name)
(enum_item
  name: (type_identifier) @enum.name) @enum

;; Trait items (capturing name)
(trait_item
  name: (type_identifier) @trait.name) @trait

;; Call expressions with direct identifier target: foo(...)
(call_expression
  function: (identifier) @call.target) @call

;; Call expressions with field/method target: obj.foo(...)
(call_expression
  function: (field_expression field: (field_identifier) @call.target)) @call

;; Call expressions with scoped identifier target: module::foo(...)
(call_expression
  function: (scoped_identifier name: (identifier) @call.target)) @call

;; Generic calls: foo::<T>(...)
(call_expression
  function: (generic_function function: (identifier) @call.target)) @call

;; Generic method calls: obj.foo::<T>(...)
(call_expression
  function: (generic_function function: (field_expression field: (field_identifier) @call.target))) @call

;; Generic scoped calls: module::foo::<T>(...)
(call_expression
  function: (generic_function function: (scoped_identifier name: (identifier) @call.target))) @call

;; Impl block items capturing the target struct/type name
(impl_item
  type: (type_identifier) @impl.target) @impl

(impl_item
  type: (generic_type type: (type_identifier) @impl.target)) @impl

;; Struct field declarations capturing member type references
(field_declaration
  type: (type_identifier) @field.type) @field

(field_declaration
  type: (generic_type type: (type_identifier) @field.type)) @field
