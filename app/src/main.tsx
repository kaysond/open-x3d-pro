import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { useStore } from './store';
import './styles/global.css';

// Outside React so StrictMode's double effects cannot subscribe twice; lives for the app's lifetime.
void useStore.getState().init();

const root = document.getElementById('root');
if (!root) throw new Error('#root element missing from index.html');
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
