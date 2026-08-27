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
      },
      "table": {
        "adjust_width": "false"
      }
    }
  }
}
@settings(format:json){
  {
    "ignore": {
      "files": [
        "00-09 System/01 Apps/obsidian"
      ]
    },
    "meta": {
      "id": {
        "type": "nanoid",
        "length": 8,
        "prefix": "doc-",
        "force": true,
        "overwrite": true
      },
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
      },
      "topics": {
        "type": "list",
        "list_type": "@link",
        "always_newline": false
      },
      "url.wiki": {
        "type": "list",
        "always_newline": true
      },
      "url.source": {
        "type": "list",
        "always_newline": true
      },
      "url.main": {
        "type": "list",
        "always_newline": true
      }
    },
    "link": {
      "no_space": "true"
    },
    "table": {
      "adjust_width": "false"
    },
    "blockquote": {
      "always_newline": true
    },
    "callout": {
      "style": {
        "content": "block"
      }
    },
    "list": {
      "multiline": {
        "style": {
          "content": "box"
        }
      }
    },
    "migration": {
      "meta": {
        "created": {
          "type": "datetime",
          "format": "iso8601"
        },
        "updated": {
          "type": "datetime",
          "format": "iso8601",
          "merge": "modified"
        }
      },
      "url": {
        "args": {
          "display": "force"
        },
        "style": "link"
      }
    }
  }
}

