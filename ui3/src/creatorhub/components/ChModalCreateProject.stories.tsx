import ChModalCreateProject from "./ChModalCreateProject";

export default {
  title: "CreatorHub/Components/Create Scene",
  component: ChModalCreateProject,
  parameters: { layout: "fullscreen" },
};

export const Default = {
  args: {
    open: true,
    initialValue: { name: "My Awesome Scene" },
  },
};

export const Empty = {
  args: {
    open: true,
    initialValue: { name: "" },
  },
};

export const Error = {
  args: {
    open: true,
    initialValue: { name: "Taken Scene" },
    takenPaths: ["taken-scene"],
  },
};
