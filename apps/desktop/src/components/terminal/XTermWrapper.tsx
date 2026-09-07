import { useEffect, useRef } from 'react';
import { Terminal } from 'xterm';
import { FitAddon } from 'xterm-addon-fit';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useProjectStore } from '../../store/projectStore';
import 'xterm/css/xterm.css';

export default function XTermWrapper() {
  const terminalRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<Terminal | null>(null);
  const sessionIdRef = useRef<string | null>(null);
  const activeProjectId = useProjectStore(state => state.activeProjectId);

  useEffect(() => {
    if (!terminalRef.current || !activeProjectId) return;

    const term = new Terminal({
      theme: {
        background: '#1e1e1e',
        foreground: '#d4d4d4',
        cursor: '#4ade80',
        selectionBackground: '#264f78',
      },
      fontFamily: 'Menlo, Monaco, "Courier New", monospace',
      fontSize: 12,
    });
    
    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(terminalRef.current);
    fitAddon.fit();
    
    xtermRef.current = term;
    
    let isMounted = true;
    let unlisten: (() => void) | null = null;

    const isRunningInTauri = typeof window !== 'undefined' && ((window as any).__TAURI_INTERNALS__ !== undefined);
    if (!isRunningInTauri) {
      term.writeln('\x1b[33m[Trans4mers Terminal Substrate]\x1b[0m');
      term.writeln('\x1b[90mNative PTY requires the desktop shell. Launch via .\\run-app.bat or cargo tauri dev.\x1b[0m');
      return;
    }

    invoke<string>('create_terminal_session', { projectId: activeProjectId, cols: term.cols, rows: term.rows })
      .then((sessionId) => {
        if (!isMounted) {
          invoke('destroy_terminal_session', { sessionId }).catch(() => {});
          return;
        }
        sessionIdRef.current = sessionId;
        
        listen<any>('domain_event', (event) => {
          try {
             const payload = typeof event.payload === 'string' ? JSON.parse(event.payload) : event.payload;
             const termOut = payload?.event?.TerminalOutput || (payload?.event?.type === 'TerminalOutput' ? payload.event.data : null);
             if (termOut && termOut.terminal_id === sessionId) {
                 term.write(termOut.data);
             }
          } catch(e) {
             console.warn('Failed to parse domain_event in XTermWrapper:', e);
          }
        }).then(u => {
          if (!isMounted) {
            u();
          } else {
            unlisten = u;
          }
        });

        term.onData((data) => {
          const encoder = new TextEncoder();
          invoke('terminal_write', { 
            sessionId: sessionId, 
            data: Array.from(encoder.encode(data)) 
          }).catch(console.error);
        });
      })
      .catch(err => term.writeln(`\x1b[31mFailed to start PTY: ${err}\x1b[0m`));

    const handleResize = () => {
      try {
        fitAddon.fit();
        if (sessionIdRef.current) {
           invoke('terminal_resize', { 
             sessionId: sessionIdRef.current, 
             cols: term.cols, 
             rows: term.rows 
           }).catch(console.error);
        }
      } catch (e) {}
    };
    window.addEventListener('resize', handleResize);

    const resizeObserver = new ResizeObserver(() => {
      handleResize();
    });
    if (terminalRef.current) {
      resizeObserver.observe(terminalRef.current);
    }

    return () => {
      isMounted = false;
      window.removeEventListener('resize', handleResize);
      resizeObserver.disconnect();
      if (unlisten) unlisten();
      if (sessionIdRef.current) {
        invoke('destroy_terminal_session', { sessionId: sessionIdRef.current }).catch(() => {});
        sessionIdRef.current = null;
      }
      term.dispose();
    };
  }, [activeProjectId]);

  return <div ref={terminalRef} className="h-full w-full overflow-hidden" />;
}
