import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

import { AppBoundary } from "./components/layout/AppBoundary";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AppBoundary>
      <App />
    </AppBoundary>
  </React.StrictMode>
);
