# new-shop - Marketplace UX Improvements
ui3 component family ported from the internal Marketplace UX Improvements Figma spec. Structure and layout kept faithful; restyled onto ui3's dark, tokenized surfaces (`--lm-*`, `--rar-*`, `--accent`, `--brand`) - the same divergence NftCard and the rest of `marketplace/` already use. Plain JSX + CSS, no new dependencies.
## Components
| File | Role | Figma source |
|---|---|---|
| `NewShopTabs` | sub-nav (Overview / All Assets / My Assets / My Favorites) | tab bar under the chrome |
| `NewShopHeroBanner` | promo banner: eyebrow, title, subtitle, orange CTA, art slot; `tone` = purple \| magenta \| neon | "Best Rated Emotes", "Week Selected Outfits", MANA splash |
| `NewShopAssetCard` | media + favourite heart + hover Buy, then name / price / timestamp / rarity chip | the asset card grid |
| `NewShopFeaturedRow` | section header (title + View all + prev/next) over a horizontal card track | "Featured" carousels |
| `NewShopFilterSidebar` | item count, On Sale switch, checkbox filter groups | browse left rail |
| `NewShopRankTable` | rank / item (thumb + verified) / floor / volume | "Top Assets" leaderboard |
| `NewShopHome` | page - tabs + banner pair + featured rows + rank table | the flagship home frame |
| `NewShopBrowse` | page - tabs + filter rail + sort toolbar + asset grid | the "All Assets" frames |

Atoms reused: `Button`, `ManaMark`, `Toggle`, `Checkbox`, `Dropdown`, `CardGrid`.

Consume from sites like the rest of `@ui/*` - import the component and its CSS:
```tsx
import NewShopHome from "@ui/marketplace/new-shop/NewShopHome";
import "@ui/marketplace/new-shop/newshophome.css";
```
Page components pull in their children's CSS transitively via the `.tsx` imports; when mounting an individual atom, import its co-located lowercase `.css`.
Interaction tests: `NewShopAssetCard.interactions.stories`, `NewShopTabs.stories`, and `NewShopBrowse.interactions.stories` carry `play` functions covering favourite toggle, hover Buy, card-open target, tab switch, On Sale toggle, sort selection, filter option, and empty state. Both runners: `npm test` (jsdom, aggregated in `src/interactions.test.tsx`) and `npm run test:browser` (real Chromium).
