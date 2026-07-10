import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { AppErrorBoundary } from "./ui/AppErrorBoundary";
import { FeedbackProvider } from "./ui/feedback";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AppErrorBoundary>
      <FeedbackProvider>
        <App />
      </FeedbackProvider>
    </AppErrorBoundary>
  </React.StrictMode>
);
