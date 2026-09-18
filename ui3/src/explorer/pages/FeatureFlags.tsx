import { useState } from "react";
import Button from "../../atoms/Button";
import { FEATURE_FLAGS, flagState, initialFeatureFlags, saveFlagOverride, type FeatureFlagId, type FlagOverride } from "../../data/featureFlags";
import "./featureflags.css";

export default function FeatureFlags() {
  const [search, setSearch] = useState("");
  const [, refresh] = useState(0);
  const [error, setError] = useState("");
  const flags = FEATURE_FLAGS.filter(flag => `${flag.name} ${flag.id} ${flag.description}`.toLowerCase().includes(search.trim().toLowerCase()));
  const pending = FEATURE_FLAGS.some(flag => flagState(flag.id).enabled !== initialFeatureFlags[flag.id]);
  function update(id: FeatureFlagId, override: FlagOverride) {
    try { saveFlagOverride(id, override); setError(""); }
    catch { setError("Your browser couldn\u2019t save this choice. Allow site storage and try again."); }
    refresh(value => value + 1);
  }
  function reset() {
    try { for (const flag of FEATURE_FLAGS) saveFlagOverride(flag.id, "default"); setError(""); }
    catch { setError("Your browser couldn\u2019t reset every flag. Allow site storage and try again."); }
    refresh(value => value + 1);
  }
  return <section className="ff" aria-labelledby="feature-flags-title">
    <h2 id="feature-flags-title">Feature flags</h2>
    <p>Choose which Explorer features to use in this browser. Default follows the current release. Reload Explorer to apply changes.</p>
    <div className="ff__toolbar"><input type="search" aria-label="Search feature flags" placeholder="Search flags" value={search} onChange={event => setSearch(event.target.value)} /><Button variant="secondary" size="sm" onClick={reset}>Reset all to default</Button></div>
    {error && <p role="alert" className="ff__error">{error}</p>}
    <div className="ff__list">{flags.map(flag => {
      const state = flagState(flag.id);
      return <div className="ff__row" key={flag.id}>
        <div><label htmlFor={`flag-${flag.id}`}>{flag.name}</label><p id={`description-${flag.id}`}>{flag.description}</p><code>{flag.id}</code><small>{state.source === "url" ? "Set by URL \u00b7 changing this replaces the URL override" : state.source === "browser" ? "Saved in this browser" : "Release default"}</small></div>
        <select id={`flag-${flag.id}`} aria-describedby={`description-${flag.id}`} value={state.override} onChange={event => update(flag.id, event.target.value as FlagOverride)}>
          <option value="default">Default ({flag.defaultEnabled ? "Enabled" : "Disabled"})</option><option value="enabled">Enabled</option><option value="disabled">Disabled</option>
        </select>
      </div>;
    })}</div>
    {!flags.length && <p role="status">No flags match &#x201c;{search}&#x201d;. Try a name or flag ID.</p>}
    {pending && <footer className="ff__apply"><span role="status">Changes saved. Reload to apply them.</span><Button onClick={() => window.location.reload()}>Reload Explorer</Button></footer>}
  </section>;
}
