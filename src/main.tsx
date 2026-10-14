import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
import { installTestApi } from "./lib/testApi";
import { applyTheme } from "./lib/theme";

applyTheme();

installTestApi();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
