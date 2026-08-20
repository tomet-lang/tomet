@meta{ type: config }
@config(format:json){
  {
    "format": {
      "meta": {
        "always_newline": true,
        "format": "yaml"
      },
      "heading": {
        "space_inside_brackets": true
      }
    }
  }
}
@settings(format:json){
  {
    "meta": {
      "aliases": {
        "type": "list",
        "always_newline": true
      },
      "created": {
        "type": "datetime",
        "format": "rfc3339",
        "offset": "+09:00"
      },
      "modified": {
        "type": "datetime",
        "format": "rfc3339",
        "offset": "+09:00"
      }
    },
    "wikilink": {
      "no_space": "true"
    },
    "migration": {
      "created": {
        "type": "datetime",
        "format": "iso8601"
      }
    }
  }
}
