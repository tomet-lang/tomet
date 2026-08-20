@meta{ type: settings }
@settings{
  elements: {
    bookmark: {
      args: {
        name: { type: string },
        count: { type: uint, default: 0 },
        variant: { type: string }
      },
      required: [ name ],
      positional: [ name ],
      content: nest(allow: [callout, codeblock]),
      placement: block,
      singleton: false
    }
  },
  types: {
    bookmark: {
      style: one_line
    },
    todo: {},
    daily-note: {
      template: "path:ja/features/templates/template.daily-note.tm"
    }
  }
}
