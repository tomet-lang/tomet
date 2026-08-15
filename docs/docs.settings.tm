@meta{ type:@settings }
@settings{
  functions: {
    bookmark: {
      input: { name: string!, count: uint = 0, variant: "a"|"b"|"c" },
      area: nested(allow: [callout, codeblock]),
      placement: block,
      singleton: false
    }
  },
  types: {
    bookmark: {
      style: one_line
    }
  }
}
