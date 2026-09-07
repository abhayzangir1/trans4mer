import { Component, ReactNode, ErrorInfo } from 'react';

interface Props {
  panelName: string;
  children?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class PanelBoundary extends Component<Props, State> {
  public state: State = {
    hasError: false,
    error: null
  };

  public static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  public componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error(`PanelBoundary (${this.props.panelName}) caught an error:`, error, errorInfo);
  }

  public render() {
    if (this.state.hasError) {
      return (
        <div className="flex h-full w-full flex-col items-center justify-center bg-background border border-destructive/50 p-4 text-center">
          <div className="text-destructive font-semibold mb-2">
            {this.props.panelName} Panel Failed
          </div>
          <button 
            className="px-3 py-1 bg-muted hover:bg-muted/80 text-muted-foreground rounded text-sm"
            onClick={() => this.setState({ hasError: false, error: null })}
          >
            Reload Panel
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}
