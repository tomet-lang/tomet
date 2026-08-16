@meta{ type:@settings }
@settings{
  elements: {
    @settings: {
      // args: target(path, file, ref)か、settings言語
      values: settings,
      placement: head,
      singleton: true
    },
    @config: {
      values: json | toml | yaml | kdl,
      placement: head,
      singleton: true
    },
    @meta: {
      values: json | toml | yaml | kdl,
      placement: head,
      singleton: true
    },
  }
}
