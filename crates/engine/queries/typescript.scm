;; ====================================================================
;; TypeScript & JavaScript Tree-sitter Queries for Repotrim
;; ====================================================================

;; Top-level function declarations
(function_declaration
  name: (identifier) @function.name
  parameters: (formal_parameters) @function.parameters
  return_type: (type_annotation)? @function.return_type
  body: (statement_block)? @function.body) @function

;; Exported function declarations
(export_statement
  declaration: (function_declaration
    name: (identifier) @function.name
    parameters: (formal_parameters) @function.parameters
    return_type: (type_annotation)? @function.return_type
    body: (statement_block)? @function.body)) @function

;; Class method definitions
(method_definition
  name: (property_identifier) @function.name
  parameters: (formal_parameters) @function.parameters
  return_type: (type_annotation)? @function.return_type
  body: (statement_block)? @function.body) @function

;; Class declarations
(class_declaration
  name: (type_identifier) @struct.name
  body: (class_body) @struct.body) @struct

;; Exported class declarations
(export_statement
  declaration: (class_declaration
    name: (type_identifier) @struct.name
    body: (class_body) @struct.body)) @struct

;; Interface declarations
(interface_declaration
  name: (type_identifier) @trait.name
  body: (interface_body) @trait.body) @trait

;; Exported interface declarations
(export_statement
  declaration: (interface_declaration
    name: (type_identifier) @trait.name
    body: (interface_body) @trait.body)) @trait

;; Type alias declarations
(type_alias_declaration
  name: (type_identifier) @enum.name) @enum

;; Exported type alias declarations
(export_statement
  declaration: (type_alias_declaration
    name: (type_identifier) @enum.name)) @enum

;; Enum declarations
(enum_declaration
  name: (identifier) @enum.name) @enum

;; Direct call expressions: func(...)
(call_expression
  function: (identifier) @call.target) @call

;; Method / member call expressions: obj.method(...)
(call_expression
  function: (member_expression
    property: (property_identifier) @call.target)) @call

;; Generic type references
(type_identifier) @field.type

;; Class heritage (extends / implements)
(class_heritage
  (extends_clause
    value: (identifier) @impl.target)) @impl
