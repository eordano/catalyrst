import FdSection, { FdPageHead } from "../components/FdSection";
import FdReturnForm, { type FdReturnFormProps } from "../components/FdReturnForm";

export type FdReturnPageProps = FdReturnFormProps;

export default function FdReturnPage(props: FdReturnPageProps) {
  return (
    <div className="fd-page fd-stack">
      <FdPageHead
        title="Coming back?"
        intro="Enter your return code to pick up your persona."
      />
      <FdSection title="Return code">
        <FdReturnForm {...props} />
      </FdSection>
    </div>
  );
}
