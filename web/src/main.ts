import "@fontsource/kalam/400.css";
import "@fontsource/kalam/700.css";
import "./ui/style.css";
import { mountApp } from "./ui/app";

const root = document.querySelector<HTMLElement>("#app");
if (!root) throw new Error("#app not found");
mountApp(root);
