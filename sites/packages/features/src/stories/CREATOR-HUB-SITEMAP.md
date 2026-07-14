# Creator Hub -- click/state sitemap

Every edge is one click, or a >100ms LOAD~ (engine boot, folder picker, signed
deploy, scaffolding). CAPS = machine states; `/paths` = routes. Verified
against the story audit.

```
/create (HOME)
+-- rail: Home - Scenes - Templates - Collections | Worlds - Land^ - Names^ - Metrics | Learn
|         (+ Curate -- committee only - Settings - Sign in)
|
+-[Start building]--~ LOAD engine ("Loading scene editor...")--> EDITOR
+-[See All]--------> /create/scenes - /create/learn
+-[Sign in]--------> SIGNIN_MODAL --[OTP/social]--> signed-in
|
+-- /create/templates
|    +-[card]--> CONFIRM_MODAL --[Create]--> /creator-hub/create-project?template=X
|                     +-[Cancel/Esc]--> back
+-- /creator-hub/create-project        (machine: create-project)
|    NAMING --[Create]-+- template preselected --~ folder picker --> SCAFFOLDING
|    (name prefilled)  +- no template --[pick]--> TEMPLATING --~--> SCAFFOLDING
|    SCAFFOLDING --~ write files --> CREATED --[Open in editor]--> EDITOR
|                        +- error --> ERROR --[Retry]/[Choose folder]--> SCAFFOLDING
|
+-- EDITOR  /creator-hub/scene-editor   (machine: scene-editor-place-items)
|    ~ BOOT > EDITING -[Open Assets]> BROWSING -[place]> PLACING -[Create entity]>
|    TRANSFORMING -[axes]> MODIFYING -[Save]~ FSA write > SAVED -[continue]> EDITING
|    EDITING -[Play]~ "Loading preview..." > PREVIEW -[Pause]<->[>]-[Stop]> EDITING
|    -[Exit]> /create - -[Publish]> deploy flow      (no auto-play; Stop != reload)
|
+-- /create/scenes -- [scene card]--~ hydrate composite --> EDITOR (reopen+continue)
|    +- [kebab > Delete] local > CONFIRM modal (opt-in file delete;
|    |    folder-less = permanence warning) -> removed + status toast
|    +- empty: [Import]/[Templates]/[Sign in]
|
+-- /create/wearables (COLLECTIIONS)
|    +-[New collection]> /collections/new   (machine: wearable-create-collection)
|    |   NAMING(Enter submits; "third-party?"->?type=linked) -> ITEMS(upload dropzone,
|    |   .zip/.glb/.gltf/.png, remove) -> REVIEW -> SUBMITTING~ -> DONE -> detail
|    +-[collection]> /collections/:id  tabs items<->activity (?tab)
|    |   +-[item row]> ITEM-EDITOR: SELECT>MODEL(name+glb)>CATEGORY>RARITY>PRICE>
|    |   |   SAVE~> SAVED -[Add another item]> SELECT - -[Back to collection/x]> detail
|    |   +-[Publish]> PUBLISH: SUMMARY>COST>TERMS>PAY~(disclosed stub)> SUBMITTED
|    +-[item]> /items/:id -[Edit]> item-editor?step=model
|
+-- WORLDS
|    +- /creator-hub/manage -[card]> settings/layout - -[Your Storage]> storage panel
|    +- storage: SELECT world > QUOTA panel (DAO-proposal link) - BUY MANA/LAND/NAME^
|    +- world-settings: tabs -[Save]~ invoke > saved - [Discard] - [Unpublish]~ real
|    +- world-permissions: tabs -[invite]/[add collaborator]/[password >=8+2num]->
|                          COMMIT~ (disclosed simulated)
|
+-- /creator-hub/deploy-world  (machine: deploy-scene)
|    PICK NAME(live; ?name= preselect) -[Deploy]~ signed deploy -> SUCCESS
|    +-[Claim name]> marketplace claim (carries project/world/origin) ->
|         [Use in Publish to World]--back w/ ?name&claimed=1 + project/world/from
|         +- name not indexed yet > PENDING "being indexed" + [Check again]
|            (never the empty-names claim funnel again)
|
+-- /creator-hub/metrics   signed-out>GATE - creator>cards (real: collections/
|    on-sale/sales/visits - per-card "Not available" on failure - EmptyState if none)
|    +- Operator tab: live presence or "Presence unavailable"
|
+-- delete-project: CONFIRM -[Delete]~ signed tombstone > done (?local=deleted|kept)
+-- /create/curate (committee): QUEUE(filters ?status ?type ?assignee=me, search,
     sort) -[row]> REVIEW -[approve/reject +comment]> DECIDED -> queue
```

Legend: `[x]` click edge - `~` >100ms invoked load (spinner/status shown) -
`<->` reversible pair - `^` external tab. Orphans/gaps (metrics-funnel,
integration-create-entry, single items) are intentionally absent -- not implemented.
