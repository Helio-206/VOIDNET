# VOID UI

Status: draft  
Layer: declarative UI document for VOIDBrowser and VOID Runtime

VOID UI is a compact declarative language for rendering VOID applications without assuming a full web platform. It describes structure, intent, and actions. The runtime decides how to render it natively and how to isolate permissions.

## Example

```vui
page {
  title "VOIDNET"

  column {
    text "A new layer has emerged."

    button {
      label "Connect"
      action "void://core/connect"
    }
  }
}
```

## Goals

- Small grammar.
- Strongly structured nodes.
- Easy parser.
- Native rendering target.
- Permission-aware actions.
- No implicit script execution.

## Non-Goals

- Replacing HTML.
- Shipping arbitrary JavaScript.
- Recreating browser tabs, DOM APIs, or CSS cascade behavior.
- Making every VOID app a web app.

## Document Model

```text
Document = Page
Page     = title? Node*
Node     = column | row | text | button | input | list | panel | image
Action   = VOID URI
```

## Core Elements

```text
page {
  title "..."
  node*
}

column {
  node*
}

row {
  node*
}

text "..."

button {
  label "..."
  action "void://..."
}

input {
  id "message"
  placeholder "Message"
  secure false
}

list {
  item { text "..." }
}
```

## Runtime Rules

- Actions must be `void://` URIs.
- Runtime authorities such as `core` may request privileged operations.
- `.void` authorities are resolved through VOID DNS.
- Apps may not access identity signing, storage, or network streams without declared permissions.
- UI documents are data, not executable code.

## Parser Direction

The first parser is implemented in `core/protocol/src/ui.rs` as a small hand-written tokenizer and recursive parser. The grammar is intentionally brace-delimited and string-literal based so it can become a stable format before any styling layer is introduced.

## Future Extensions

- `permission` declaration blocks.
- `state` bindings with explicit runtime events.
- `stream` bindings for live data.
- Compact binary representation for transmitted UI.
