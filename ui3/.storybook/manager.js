import { addons } from "storybook/manager-api";

addons.setConfig({
        sidebar: {
                showRoots: false,
        },
});

if (typeof document !== "undefined") {
        const style = document.createElement("style");
        style.textContent = `
    .sidebar-item[data-item-id="web"],
    [data-nodetype][data-item-id="web"] {
      margin-top: 12px !important;
      padding-top: 12px !important;
      border-top: 1px solid rgba(150, 150, 170, 0.22) !important;
    }
  `;
        document.head.appendChild(style);
}
