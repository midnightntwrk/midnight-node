#ci #node #toolkit
# Identify release images by the public release commit

Build main/release node and toolkit images with full commit SHA tags and OCI
revision metadata. Before release promotion, require that both architectures were
built at the commit selected for the public release. PR CI keeps tree-based image
reuse, while release binaries retain the release commit in their version output.

Existing releases are unchanged. Release branches must adopt the updated build
workflow and Earthfile before producing artifacts for the new release workflow.
