import React from "react";
import ReactDOM from "react-dom/client";
import { ConsoleApp } from "./ConsoleApp";
import "../styles/index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ConsoleApp />
  </React.StrictMode>,
);
