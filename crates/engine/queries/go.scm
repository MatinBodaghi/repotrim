;; ====================================================================
;; Go Tree-sitter Queries for Repotrim Symbol and Reference Extraction
;; ====================================================================

;; Top-level function declarations
(function_declaration
  name: (identifier) @function.name
  parameters: (parameter_list) @function.parameters
  result: (_)? @function.return_type
  body: (block)? @function.body) @function

;; Method declarations with receiver (e.g. func (s *Server) Handle(...) ...)
(method_declaration
  receiver: (parameter_list) @function.receiver
  name: (field_identifier) @function.name
  parameters: (parameter_list) @function.parameters
  result: (_)? @function.return_type
  body: (block)? @function.body) @function

;; Struct type declarations (e.g. type Config struct { ... })
(type_spec
  name: (type_identifier) @struct.name
  type: (struct_type) @struct.body) @struct

;; Interface type declarations (e.g. type Reader interface { ... })
(type_spec
  name: (type_identifier) @trait.name
  type: (interface_type) @trait.body) @trait

;; Type alias / custom type declarations (e.g. type ID string)
(type_spec
  name: (type_identifier) @enum.name
  type: (type_identifier)) @enum

;; Call expressions with direct identifier target: foo(...)
(call_expression
  function: (identifier) @call.target) @call

;; Call expressions with selector/method target: obj.Method(...) or pkg.Function(...)
(call_expression
  function: (selector_expression field: (field_identifier) @call.target)) @call

;; Struct field declarations with type references
(field_declaration
  type: (type_identifier) @field.type) @field

(field_declaration
  type: (pointer_type (type_identifier) @field.type)) @field

(field_declaration
  type: (slice_type (type_identifier) @field.type)) @field
