import { Component, ReactNode, ErrorInfo } from 'react';

interface Props {
  name: string;
  children?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class WidgetBoundary extends Component<Props, State> {
  public state: State = {
    hasError: false,
    error: null
  };

  public static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  public componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error(`WidgetBoundary (${this.props.name}) caught an error:`, error, errorInfo);
  }

  public render() {
    if (this.state.hasError) {
      return (
        <div className="p-2 text-muted-foreground text-sm italic border border-destructive/20 rounded m-2">
          {this.props.name} unavailable: {this.state.error?.message}
        </div>
      );
    }

    return this.props.children;
  }
}
