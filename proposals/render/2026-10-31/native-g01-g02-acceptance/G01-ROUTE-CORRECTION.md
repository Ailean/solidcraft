# G01 PNG navigation correction

SOURCE_ONLY / NATIVE_NOT_RUN. PNG format and Transparent background are in **Output > Image**, not Destination. **Output > Destination** contains completed-image export/copy controls.

Source: render_workspace_inspector.rs:788-794,877-895,915-930 and render_workspace_studio.rs:411-435, with exact identities in source-bindings.json. G01-CONTROL-MAP.md incorporates this correction. No setting, specimen, assertion or native-case result changes.
