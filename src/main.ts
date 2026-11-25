import { mount } from "svelte";
// Bundled fonts (SIL OFL, see public/licenses/). Nothing is loaded from the internet.
import "@fontsource-variable/inter";
import "@fontsource-variable/source-serif-4";
import "./styles/tokens.css";
import "./styles/base.css";
import App from "./App.svelte";

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
