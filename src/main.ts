import './styles/app.css';
import { mount } from 'svelte';
import App from './App.svelte';
import { setupI18n } from './lib/i18n';

async function start(): Promise<void> {
  // App renders translated loading text before its onMount bootstrap runs.
  await setupI18n('en');
  const target = document.getElementById('app');
  if (!target) throw new Error('missing #app element');
  mount(App, { target });
}

void start();
