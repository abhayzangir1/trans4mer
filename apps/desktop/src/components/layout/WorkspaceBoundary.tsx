import { Component, ReactNode, ErrorInfo } from 'react';

interface Props {
  children?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class WorkspaceBoundary extends Component<Props, State> {
  public state: State = {
    hasError: false,
    error: null
  };

  public static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  public componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error('WorkspaceBoundary caught an error:', error, errorInfo);
  }

  public render() {
    if (this.state.hasError) {
      return (
        <div className="flex h-full w-full flex-col items-center justify-center bg-background text-foreground p-8">
          <h2 className="text-xl font-bold text-destructive mb-4">Workspace Crashed</h2>
          <p className="mb-4">The current workspace layout encountered a fatal error.</p>
          <pre className="p-4 bg-muted text-muted-foreground rounded text-sm w-full overflow-auto">
            {this.state.error?.message}
          </pre>
          <button 
            className="mt-6 px-4 py-2 bg-secondary text-secondary-foreground rounded hover:bg-secondary/80"
            onClick={() => this.setState({ hasError: false, error: null })}
          >
            Attempt Workspace Recovery
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}
