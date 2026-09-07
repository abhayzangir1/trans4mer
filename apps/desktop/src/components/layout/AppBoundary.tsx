import { Component, ReactNode, ErrorInfo } from 'react';

interface Props {
  children?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class AppBoundary extends Component<Props, State> {
  public state: State = {
    hasError: false,
    error: null
  };

  public static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  public componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error('AppBoundary caught an error:', error, errorInfo);
  }

  public render() {
    if (this.state.hasError) {
      return (
        <div className="flex h-screen w-screen flex-col items-center justify-center bg-background text-destructive">
          <h1 className="text-2xl font-bold mb-4">CRITICAL APPLICATION FAILURE</h1>
          <p className="mb-4">The Trans4mers desktop shell has crashed.</p>
          <pre className="p-4 bg-muted text-muted-foreground rounded text-sm max-w-2xl overflow-auto">
            {this.state.error?.message}
          </pre>
          <button 
            className="mt-6 px-4 py-2 bg-primary text-primary-foreground rounded hover:bg-primary/90"
            onClick={() => window.location.reload()}
          >
            Restart Shell
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}
