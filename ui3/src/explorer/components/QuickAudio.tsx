import { useEngineSettings } from "../../overlay/engineSettings";
import Slider from "../../atoms/Slider";

const CHANNELS = [
  ["Master Volume", "Master"], ["Scene Volume", "In-World Music & SFX"],
  ["System Volume", "UI SFX"], ["Avatar Volume", "Avatar & Emotes SFX"],
  ["Voice Volume", "Voice Chat & Streams"],
] as const;

export default function QuickAudio() {
  const { info, values, setValue, connected } = useEngineSettings();
  return <div className="sd__audio">
    {!info && <p role="status">{connected === false ? "Audio controls are unavailable until the engine connects." : "Loading audio controls\u2026"}</p>}
    {CHANNELS.map(([name, label]) => {
      const setting = info?.[name];
      return <div key={name}><label>{label}</label>
        <Slider ariaLabel={label} format={value => `${Math.round(value)}%`} min={setting?.minValue ?? 0} max={setting?.maxValue ?? 100} step={setting?.stepSize ?? 1} value={values[name] ?? 100} disabled={!setting} onChange={value => setValue(name, value)} />
      </div>;
    })}
  </div>;
}
