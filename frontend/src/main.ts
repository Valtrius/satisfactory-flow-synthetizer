import './style.css';
import App from './App.svelte';
import { mount } from 'svelte';
import { prepareOfflineDocument } from './lib/offline/bootstrap';
import { hideWebLoadProgress, installWebLoadProgressListener } from './lib/offline/progress';

const target = document.getElementById('app')!;
installWebLoadProgressListener();
void prepareOfflineDocument().then((ready) => {
  if (!ready) return;
  target.replaceChildren();
  mount(App, { target });
  hideWebLoadProgress();
});
