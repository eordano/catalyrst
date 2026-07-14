import type { CameraPrefs } from "../types";
import Modal from "../../components/Modal";
import Slider from "../../atoms/Slider";
import Checkbox from "../../atoms/Checkbox";
import { DEFAULT_CAMERA_PREFS, normalizeCameraPrefs } from "../camera-prefs";
import "./decamerasettings.css";

const PRESET_OPTIONS = [
  { id: "blender", label: "Blender" },
  { id: "blender-lmb", label: "Blender (LMB)" },
  { id: "maya", label: "Maya" },
];

const CHEATSHEET = {
  blender: [
    ["Orbit", "Middle-drag"],
    ["Pan", "Shift + Middle-drag"],
    ["Dolly", "Ctrl + Middle-drag / Wheel"],
    ["Focus selection", "F"],
    ["Views", "Numpad 1 / 3 / 7 (Ctrl = opposite)"],
    ["Ortho toggle", "Numpad 5 (coming soon)"],
    ["Fly / orbit toggle", "`  (backtick)"],
    ["Fly move", "W A S D + Space / Shift"],
  ],
  "blender-lmb": [
    ["Orbit", "Middle-drag  \u{B7}  Alt + Left-drag"],
    ["Pan", "Shift + Middle  \u{B7}  Alt + Shift + Left"],
    ["Dolly", "Ctrl + Middle  \u{B7}  Alt + Ctrl + Left  \u{B7}  Wheel"],
    ["Focus selection", "F"],
    ["Views", "Numpad 1 / 3 / 7 (Ctrl = opposite)"],
    ["Ortho toggle", "Numpad 5 (coming soon)"],
    ["Fly / orbit toggle", "`  (backtick)"],
  ],
  maya: [
    ["Orbit", "Alt + Left-drag"],
    ["Pan", "Alt + Middle-drag"],
    ["Dolly", "Alt + Right-drag  \u{B7}  Wheel"],
    ["Focus selection", "F"],
    ["Views", "Numpad 1 / 3 / 7 (Ctrl = opposite)"],
    ["Ortho toggle", "Numpad 5 (coming soon)"],
  ],
};

interface SensitivitySliderProps {
  label: string;
  value: number;
  onChange: (value: number) => void;
}

function SensitivitySlider({ label, value, onChange }: SensitivitySliderProps) {
  return (
    <div className="decam-slider">
      <span className="decam-slider-label">{label}</span>
      <Slider
        value={value}
        min={0.1}
        max={3}
        step={0.05}
        ariaLabel={label}
        format={(v) => v.toFixed(2) + "\u{D7}"}
        onChange={onChange}
      />
    </div>
  );
}

export interface DeCameraSettingsProps {
  prefs: CameraPrefs;
  onChange?: (next: CameraPrefs) => void;
  onReset?: (next: CameraPrefs) => void;
  onClose?: () => void;
}

export default function DeCameraSettings({ prefs, onChange, onReset, onClose }: DeCameraSettingsProps) {
  const p = normalizeCameraPrefs(prefs);
  const set = (patch: Partial<CameraPrefs>) => onChange?.(normalizeCameraPrefs({ ...p, ...patch }));
  const setSens = (key: "orbit" | "pan" | "zoom", v: number) =>
    set({ sensitivity: { ...p.sensitivity, [key]: v } });
  const rows = CHEATSHEET[p.preset as keyof typeof CHEATSHEET] || CHEATSHEET.blender;

  return (
    <Modal onClose={onClose} width={420} ariaLabel="Camera settings">
      <div className="decam">
        <h2 className="decam-title">Camera controls</h2>

        <label className="decam-field">
          <span className="decam-field-label">Preset</span>
          <select
            className="decam-select"
            value={p.preset}
            onChange={(e) => set({ preset: e.target.value })}
          >
            {PRESET_OPTIONS.map((o) => (
              <option key={o.id} value={o.id}>
                {o.label}
              </option>
            ))}
          </select>
        </label>

        <div className="decam-sliders">
          <SensitivitySlider label="Orbit sensitivity" value={p.sensitivity.orbit} onChange={(v) => setSens("orbit", v)} />
          <SensitivitySlider label="Pan sensitivity" value={p.sensitivity.pan} onChange={(v) => setSens("pan", v)} />
          <SensitivitySlider label="Zoom sensitivity" value={p.sensitivity.zoom} onChange={(v) => setSens("zoom", v)} />
        </div>

        <div className="decam-check">
          <Checkbox checked={p.invertY} onChange={(next) => set({ invertY: next })}>
            Invert Y (pitch)
          </Checkbox>
        </div>

        <div className="decam-cheatsheet">
          <div className="decam-cheatsheet-title">Bindings</div>
          {rows.map(([action, keys]) => (
            <div className="decam-cheat-row" key={action}>
              <span className="decam-cheat-action">{action}</span>
              <span className="decam-cheat-keys">{keys}</span>
            </div>
          ))}
        </div>

        <div className="decam-actions">
          <button
            type="button"
            className="decam-btn"
            onClick={() => onReset?.(normalizeCameraPrefs(DEFAULT_CAMERA_PREFS))}
          >
            Reset to defaults
          </button>
          <button type="button" className="decam-btn decam-btn-primary" onClick={onClose}>
            Done
          </button>
        </div>
      </div>
    </Modal>
  );
}
