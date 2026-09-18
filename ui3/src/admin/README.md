# Admin console vocabulary

Every admin, operator and What's On admin page imports `admin.css` and uses only
these classes plus ui3 atoms. `admin-css.test.ts` gates it: no raw colors, no
class outside the `adm` namespace, no page stylesheet redefining a shared rule.

## Frame

- `adm` root, `adm__nav` (console pills), `adm__page` (gradient, page padding),
  `adm__inner` (`--mid` 960px, `--narrow` 720px), `adm__head`, `adm__title`,
  `adm__sub`, `adm__h2`, `adm__h3`, `adm__tools`.
- `adm-tabs` sticky bar: `adm-tabs__list`, `adm-tabs__tab` (`is-active` or
  `aria-current="page"`).

## Surfaces

- `adm-card` (`--dashed`, `--solid`, `--link`, `--float`; `is-active`):
  `adm-card__head`, `adm-card__title`, `adm-card__text`, `adm-card__foot`.
- `adm-notice` + `data-tone="ok|warn|bad|info"`: `adm-notice__title`.
- `adm-scroll` > `adm-table`: cells take `is-center`, `is-num`, `is-nowrap`,
  `is-empty`; rows take `is-link`.
- `adm-stats` (`--grid`) is a `dl`; `adm-kpi__n` / `adm-kpi__l`; `adm-bar` with
  `span[data-tone]` fills; `adm-thumb`; `adm-grid` (`--adm-col` sets the
  minimum column); `adm-list`; `adm-pre`.

## Controls

- Buttons are the ui3 `Button` (`variant`, `size`, `tone="danger|success|warning"`).
- Search is `SearchField`; switches are `Toggle`; dialogs are `Modal`; empty
  states are `EmptyState`; spinners are `Spinner`; avatars are `Avatar`.
- `adm-pills` > `adm-pill` (`is-active` or `aria-current`, `adm-pill__count`).
- `adm-choice` (`is-active`, `data-tone`, `adm-choice__hint`) for decisions.
- `adm-field` (`is-error`) > `adm-field__label`, `adm-input`, `adm-field__help`;
  `adm-check` for a labelled checkbox row.
- `adm-actions` (`--start`, `--split`), `adm-back`, `adm-toast[data-tone]`,
  `adm-gate`.

## Text

`adm-mono`, `adm-dim`, `adm-ok`, `adm-warn`, `adm-bad`, `adm-link`; inline
`code` is styled under `.adm`.

## Page-local rules

Only for layout the shared set cannot express. Name them
`adm-<page>__<part>`, keep them in `<page>.css` next to the component, and use
tokens from `primitives.css` for every color.
