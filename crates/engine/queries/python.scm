;; ====================================================================
;; Python Tree-sitter Queries for Repotrim Symbol and Reference Extraction
;; ====================================================================

;; Top-level and method function definitions
(function_definition
  name: (identifier) @function.name
  parameters: (parameters) @function.parameters
  return_type: (type)? @function.return_type
  body: (block) @function.body) @function

;; Decorated function definitions
(decorated_definition
  definition: (function_definition
    name: (identifier) @function.name
    parameters: (parameters) @function.parameters
    return_type: (type)? @function.return_type
    body: (block) @function.body)) @function

;; Class definitions
(class_definition
  name: (identifier) @struct.name
  body: (block) @struct.body) @struct

;; Decorated class definitions
(decorated_definition
  definition: (class_definition
    name: (identifier) @struct.name
    body: (block) @struct.body)) @struct

;; Call expressions with direct identifier target: func(...)
(call
  function: (identifier) @call.target) @call

;; Call expressions with attribute/method target: obj.method(...)
(call
  function: (attribute attribute: (identifier) @call.target)) @call

;; Type annotations on parameters or variables: x: CustomType
(type
  (identifier) @field.type) @field

;; Class inheritance base types
(class_definition
  superclasses: (argument_list
    (identifier) @impl.target)) @impl
