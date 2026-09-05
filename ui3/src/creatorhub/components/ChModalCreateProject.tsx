import { useCallback, useState, type ChangeEvent } from "react";
import Modal from "../../components/Modal";
import Button from "../../atoms/Button";
import "./chmodalcreateproject.css";

const COPY = {
  title: "Create Scene",
  name: "Scene Name",
  create: "Create",
  cancel: "Cancel",
  saveHint:
    "Create asks where to save your project folder; browsers without folder access download it as a zip.",
  nameError:
    "A scene with that name already exists. Please choose a different name.",
  nameRequired: "Scene name is required.",
};

function deriveSlug(name: string): string {
  return (
    name
      .toLowerCase()
      .trim()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "new-scene"
  );
}

interface ProjectValue {
  name: string;
  path: string;
}

interface ChModalCreateProjectProps {
  open?: boolean;
  initialValue?: { name?: string };
  onClose?: () => void;
  onSubmit?: (value: ProjectValue) => void;
  onChange?: (value: ProjectValue) => void;
  takenPaths?: string[];
  error?: string | null;
  submitting?: boolean;
}

export default function ChModalCreateProject({
  open = true,
  initialValue = { name: "My Awesome Scene" },
  onClose = () => {},
  onSubmit = () => {},
  onChange = undefined,
  takenPaths = [],
  error: externalError = null,
  submitting = false,
}: ChModalCreateProjectProps) {
  const [name, setName] = useState(initialValue.name ?? "");
  const [error, setError] = useState<string | null>(null);

  const handleChange = useCallback(
    (event: ChangeEvent<HTMLInputElement>) => {
      setError(null);
      const next = event.target.value;
      setName(next);
      onChange?.({ name: next, path: deriveSlug(next) });
    },
    [onChange],
  );

  const handleSubmit = useCallback(() => {
    if (!name.trim()) {
      setError(COPY.nameRequired);
      return;
    }
    const slug = deriveSlug(name);
    if (takenPaths.includes(slug)) {
      setError(COPY.nameError);
      return;
    }
    onSubmit({ name, path: slug });
  }, [name, onSubmit, takenPaths]);

  if (!open) return null;

  const nameEmpty = !name.trim();
  const shownError = error ?? externalError;

  return (
    <Modal
      width={900}
      className="chmcp"
      ariaLabelledBy="chmcp-title"
      onClose={onClose}
      closeOnBackdrop={false}
    >
      <h2 className="chmcp__title" id="chmcp-title">{COPY.title}</h2>

      <div className="chmcp__form">
        <label className="chmcp__label" htmlFor="chmcp-name">{COPY.name}</label>
        <div className="chmcp__input">
          <input
            id="chmcp-name"
            className="chmcp__field"
            type="text"
            value={name}
            onChange={handleChange}
          />
        </div>

        <p className="chmcp__hint">{COPY.saveHint}</p>

        {shownError && (
          <p className="chmcp__error" role="alert">
            {shownError}
          </p>
        )}
      </div>

      <div className="chmcp__actions">
        <Button variant="secondary" size="lg" onClick={onClose}>
          {COPY.cancel}
        </Button>
        <Button
          variant="primary"
          size="lg"
          disabled={nameEmpty || submitting}
          onClick={handleSubmit}
        >
          {COPY.create}
        </Button>
      </div>
    </Modal>
  );
}
