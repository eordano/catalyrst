import Modal from "../../components/Modal";
import { shortcutGroups } from "../shortcuts";

export default function DeShortcutsOverlay({
  preset = "blender",
  onClose,
}: {
  preset?: string;
  onClose: () => void;
}) {
  return (
    <Modal onClose={onClose} width={620} ariaLabel="Keyboard and mouse shortcuts">
      <div className="eui-shortcuts">
        <div className="eui-shortcuts-title">Keyboard &amp; mouse shortcuts</div>
        <div className="eui-shortcuts-grid">
          {shortcutGroups({ preset }).map((group) => (
            <div key={group.title} className="eui-shortcuts-group">
              <div className="eui-menu-label">{group.title}</div>
              {group.items.map((s) => (
                <div key={s.combo + s.label} className="eui-shortcut-row">
                  <span className="eui-shortcut-label">{s.label}</span>
                  <kbd className="eui-kbd">{s.combo}</kbd>
                </div>
              ))}
            </div>
          ))}
        </div>
        <div className="eui-shortcuts-foot">
          Press <kbd className="eui-kbd">?</kbd> any time to toggle this list. Tool keys
          stay with the engine while flying or playing.
        </div>
      </div>
    </Modal>
  );
}
