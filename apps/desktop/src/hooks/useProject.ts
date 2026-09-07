import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface Project {
  id: string;
  name: string;
  workspace_path: string;
  description: string;
  global_instructions: string;
}

export function useProject(projectId: string | null) {
  const [project, setProject] = useState<Project | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!projectId) {
      setProject(null);
      setError(null);
      setLoading(false);
      return;
    }

    let isMounted = true;
    setLoading(true);

    invoke<Project>('get_project', { projectId })
      .then((data) => {
        if (isMounted) {
          setProject(data);
          setError(null);
        }
      })
      .catch((err) => {
        if (isMounted) setError(String(err));
      })
      .finally(() => {
        if (isMounted) setLoading(false);
      });

    return () => {
      isMounted = false;
    };
  }, [projectId]);

  return { project, loading, error };
}
