import { Component } from "react";
import type { ErrorInfo, ReactNode } from "react";
import { api } from "../api";

const errorBoundaryCopy = {
  title: "CodexHub could not render / CodexHub 无法完成渲染",
  body: "A sanitized failure record was created when storage was available. No stack trace or secret value is shown. / 存储可用时已创建脱敏故障记录；此处不会显示堆栈或敏感值。",
  reload: "Reload / 重新加载",
  reportFailed: "The desktop task store was unavailable. / 桌面任务存储不可用。"
};

type State = { failed: boolean; reportFailed: boolean };

export class AppErrorBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { failed: false, reportFailed: false };

  static getDerivedStateFromError(): State {
    return { failed: true, reportFailed: false };
  }

  componentDidCatch(_error: Error, _info: ErrorInfo) {
    void api.recordFrontendError("React render failure.").catch(() => {
      this.setState({ reportFailed: true });
    });
  }

  render() {
    if (!this.state.failed) return this.props.children;
    return (
      <main className="errorBoundary" role="alert">
        <h1>{errorBoundaryCopy.title}</h1>
        <p>{errorBoundaryCopy.body}</p>
        {this.state.reportFailed ? <p>{errorBoundaryCopy.reportFailed}</p> : null}
        <button className="primaryButton" type="button" onClick={() => window.location.reload()}>{errorBoundaryCopy.reload}</button>
      </main>
    );
  }
}
