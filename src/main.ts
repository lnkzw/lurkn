import { mount } from 'svelte';

import ActivityWindow from './ActivityWindow.svelte';
import App from './App.svelte';
import SettingsWindow from './SettingsWindow.svelte';
import './app.css';

const target = document.getElementById('app');
if (!target) {
  throw new Error('missing #app mount point');
}

// One bundle, several windows: settings and the activity feed are opened by
// Rust under `?view=…` (src-tauri/src/commands/window.rs).
const view = new URLSearchParams(window.location.search).get('view');
const root = view === 'settings' ? SettingsWindow : view === 'activity' ? ActivityWindow : App;

export default mount(root, { target });
