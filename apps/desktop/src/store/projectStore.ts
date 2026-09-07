import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

export interface Project {
  id: string;
  name: string;
  workspace_path: string;
  created_at: string;
}

interface ProjectState {
  projects: Project[];
  activeProjectId: string | null;
  fetchProjects: () => Promise<void>;
  createProject: (name: string, path: string) => Promise<void>;
  deleteProject: (id: string) => Promise<void>;
  setActiveProject: (id: string) => void;
}

export const useProjectStore = create<ProjectState>((set) => ({
  projects: [],
  activeProjectId: null,
  fetchProjects: async () => {
    try {
      const projects = await invoke<Project[]>('list_projects');
      set((state) => ({
        projects,
        activeProjectId: state.activeProjectId || (projects.length > 0 ? projects[0].id : null),
      }));
    } catch (e) {
      console.error('Failed to fetch projects', e);
    }
  },
  createProject: async (name, path) => {
    try {
      const newProject = await invoke<Project>('create_project', {
        name,
        workspacePath: path,
        description: null
      });
      set((state) => ({
        projects: [...state.projects, newProject],
        activeProjectId: newProject.id
      }));
    } catch (e) {
      console.error('Failed to create project', e);
      throw e;
    }
  },
  deleteProject: async (id) => {
    try {
      await invoke('delete_project', { projectId: id });
      set((state) => {
        const remaining = state.projects.filter((p) => p.id !== id);
        return {
          projects: remaining,
          activeProjectId: state.activeProjectId === id ? (remaining.length > 0 ? remaining[0].id : null) : state.activeProjectId,
        };
      });
    } catch (e) {
      console.error('Failed to delete project', e);
      throw e;
    }
  },
  setActiveProject: (id) => {
    set({ activeProjectId: id });
  },
}));
