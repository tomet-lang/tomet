@meta{ type:@settings }
@settings{
  elements: {
    bookmark: {
      args: { name: string!, count: uint = 0, variant: "a"|"b"|"c" },
      content: nested(allow: [callout, codeblock]),
      placement: block,
      singleton: false
    }
  },
  types: {
    bookmark: {
      style: one_line
    },
    daily-note: {
      template: "path:ja/features/templates/template.daily-note.tm"
    }
  }
}
