// Serveur statique minimal pour travailler l'interface dans un navigateur
// (mode démo, sans Tauri ni service). Usage : node scripts/serve-ui.mjs [port]
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../ui/', import.meta.url));
const port = Number(process.argv[2] ?? 5173);
const types = { '.html': 'text/html', '.css': 'text/css', '.js': 'text/javascript', '.png': 'image/png', '.svg': 'image/svg+xml' };

createServer(async (req, res) => {
  const path = normalize(decodeURIComponent(new URL(req.url, 'http://x').pathname)).replace(/^([\\/]\.\.)+/, '');
  // Les patch notes (CHANGELOG.md à la racine du dépôt) sont intégrées à l'application.
  const file = /^[\\/]CHANGELOG\.md$/.test(path)
    ? fileURLToPath(new URL('../CHANGELOG.md', import.meta.url))
    : join(root, path.endsWith('/') || path.endsWith('\\') ? 'index.html' : path);
  if (!file.startsWith(root) && !file.endsWith('CHANGELOG.md')) { res.writeHead(403).end(); return; }
  try {
    const body = await readFile(file);
    res.writeHead(200, { 'content-type': types[extname(file)] ?? 'application/octet-stream', 'cache-control': 'no-store' }).end(body);
  } catch {
    res.writeHead(404).end('not found');
  }
}).listen(port, '127.0.0.1', () => console.log(`UI de démo : http://localhost:${port}`));
