import { useState } from "react";
import CreatorHubChrome, { CreatorHubChromeMaybe } from "../frames/CreatorHubChrome";
import Modal from "../../components/Modal";
import Button from "../../atoms/Button";
import Checkbox from "../../atoms/Checkbox";
import "./chmodaldeleteproject.css";

const COPY = {
  title: (t: string) => `Are you sure you want to delete ${t} from 'My Scenes'?`,
  localConsequence:
    "This removes the scene from 'My Scenes' in this browser. Files on your computer are kept unless you also tick the box below.",
  networkConsequence:
    "This also unpublishes the scene from this network (catalyst.example.com): its live deployment is removed from the catalyst, so visitors will no longer find it \u{2014} Genesis City on decentraland.org is not affected. Files on your computer are kept unless you also tick the box below.",
  filesCheckbox: "Also delete this scene's files from my computer",
  filesWarning:
    "Deleting scene files from your computer is permanent, you won't be able to access this scene again.",
  cancel: "Cancel",
  confirm: "Confirm",
};

interface Project {
  id: string;
  title: string;
  path: string;
}

interface ChModalDeleteProjectProps {
  open?: boolean;
  project?: Project;
  deleteFiles?: boolean;
  chrome?: boolean;
  showFilesOption?: boolean;
  note?: string;
  /** Forwarded to `Modal`. `false` renders the dialog in place instead of portalling it. */
  portal?: boolean;
  onClose?: () => void;
  onSubmit?: (project: Project, deleteFiles: boolean) => void;
  onToggleFiles?: (checked: boolean) => void;
}

export default function ChModalDeleteProject({
  open = true,
  project = { id: "", title: "", path: "" },
  deleteFiles = false,
  chrome = true,
  showFilesOption = true,
  note = undefined,
  portal = true,
  onClose = () => {},
  onSubmit = () => {},
  onToggleFiles = undefined,
}: ChModalDeleteProjectProps) {
  const [shouldDeleteFiles, setShouldDeleteFiles] = useState(deleteFiles);

  const handleToggleFiles = (checked: boolean) => {
    setShouldDeleteFiles(checked);
    onToggleFiles?.(checked);
  };

  if (!open) {
    return chrome ? <CreatorHubChrome active="scenes" /> : null;
  }

  const handleSubmit = () =>
    onSubmit(project, showFilesOption && shouldDeleteFiles);

  const localOnly = project.id.startsWith("local:");

  const modal = (
    <Modal
      width={540}
      className="chmodaldeleteproject"
      ariaLabel={COPY.title(project.title)}
      onClose={onClose}
      showClose={false}
      portal={portal}
    >
      <h5 className="chmodaldeleteproject__title">
        {COPY.title(project.title)}
      </h5>

      <div className="chmodaldeleteproject__content">
        <p className="chmodaldeleteproject__consequence">
          {localOnly ? COPY.localConsequence : COPY.networkConsequence}
        </p>

        {note ? (
          <p
            className="chmodaldeleteproject__delete-files-warning"
            style={{ margin: "0 0 4px" }}
          >
            {note}
          </p>
        ) : null}

        {showFilesOption ? (
          <Checkbox checked={shouldDeleteFiles} onChange={handleToggleFiles}>
            {COPY.filesCheckbox}
          </Checkbox>
        ) : null}

        {showFilesOption && shouldDeleteFiles && (
          <p className="chmodaldeleteproject__delete-files-warning">
            {COPY.filesWarning}
          </p>
        )}
      </div>

      <div className="chmodaldeleteproject__actions">
        <Button variant="secondary" size="lg" onClick={onClose}>
          {COPY.cancel}
        </Button>
        <Button variant="primary" size="lg" onClick={handleSubmit}>
          {COPY.confirm}
        </Button>
      </div>
    </Modal>
  );

  return (
    <CreatorHubChromeMaybe chrome={chrome} active="scenes">
      {modal}
    </CreatorHubChromeMaybe>
  );
}
