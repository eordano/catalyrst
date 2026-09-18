import { useState } from "react";
import Button from "../../atoms/Button";
import AudioMixer from "./AudioMixer";
import { useEngineSettings } from "../../overlay/engineSettings";
import Slider from "../../atoms/Slider";

const CHANNELS = [
  ["Master Volume", "Master"], ["Scene Volume", "Scene volume"],
  ["System Volume", "Menus and buttons"], ["Avatar Volume", "Avatar and emote sounds"],
  ["Voice Volume", "Voice chat"], ["Stream Volume", "Audio and video streams"],
] as const;

export default function QuickAudio() {
  const [advanced, setAdvanced] = useState(false);
  const { info, values, setValue, connected } = useEngineSettings();
  return <div className="sd__audio">
    {!info && <p role="status">{connected === false ? "Audio controls are unavailable until the engine connects." : "Loading audio controls\u2026"}</p>}
    {!advanced && CHANNELS.map(([name, label]) => {
      const setting = info?.[name];
      return <div key={name}><label>{label}</label>
        <Slider ariaLabel={label} format={value => `${Math.round(value)}%`} min={setting?.minValue ?? 0} max={setting?.maxValue ?? 100} step={setting?.stepSize ?? 1} value={values[name] ?? 100} disabled={!setting} onChange={value => setValue(name, value)} />
      </div>;
    })}
    <Button variant="secondary" className="sd__audio-mode" aria-pressed={advanced} onClick={() => setAdvanced(value => !value)}>{advanced ? "Basic sound controls" : "Advanced sound controls"}</Button>
    <AudioMixer advanced={advanced} />
  </div>;
}
