`authoring-schemas.json` is generated from the editor scene's installed `@dcl/asset-packs`, `@dcl/ecs`, and `@dcl/protocol` packages. It supplies one action, trigger, component, and enum vocabulary to the guided interaction composer and inspector. SDK protobuf fields come from the upstream `.proto` files, including nested messages and oneof variants.

From `catalyrst/ui3`, run `npm run gen:authoring-schemas` after updating the editor scene dependencies; `npm run gen:authoring-schemas:check` detects drift. The versions are recorded in the generated JSON. Do not edit the JSON by hand.
