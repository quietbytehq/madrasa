import React from "react";
import ReactDOM from "react-dom/client";
import { HashRouter } from "react-router-dom";
import { ParentApp } from "./App";
import "./styles.css";

// A hash router: the app's pages are files inside the app, with no server to route paths.
ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <HashRouter>
      <ParentApp />
    </HashRouter>
  </React.StrictMode>,
);
