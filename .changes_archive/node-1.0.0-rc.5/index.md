Git tag: [{{ release_tag }}](https://github.com/midnightntwrk/midnight-node/tree/{{ release_tag }})

## Components
{{#if node_version}}
- 📦 `node-{{ node_version }}`
{{/if}}
{{#if toolkit_version}}
- 🧰 `toolkit-{{ toolkit_version }}`
{{/if}}
{{#if runtime_version}}
- ⚙️ `runtime-{{ runtime_version }}`
{{/if}}
