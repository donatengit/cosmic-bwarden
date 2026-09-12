import net from 'net';
import path from 'path';
import { execSync } from 'child_process';

/**
 * Parses the accumulated RDP buffer and returns the first JSON frame carrying
 * an `addon`/`addons` field (or null). Frames are length-prefixed and may be
 * split across data events, so the buffer may hold several of them.
 */
function parseFrames(buffer) {
  const fromBrace = buffer.slice(buffer.indexOf('{'));
  try {
    return JSON.parse(fromBrace);
  } catch {
    for (const m of fromBrace.matchAll(/\d+:(\{.*?\})(?=\d+:|$)/gs)) {
      try {
        const f = JSON.parse(m[1]);
        if (f && (f.addon || f.addons)) return f;
      } catch { /* not a complete frame yet */ }
    }
  }
  return null;
}

/**
 * Loads a Firefox addon temporarily via the remote debugging port.
 */
export async function loadFirefoxAddon(port, addonPath) {
  const addonAbsPath = path.resolve(addonPath);
  console.log(`Connecting to Firefox on port ${port} to install: ${addonAbsPath}`);

  // Retry logic for connection
  let socket;
  for (let i = 0; i < 20; i++) {
    try {
      socket = await new Promise((resolve, reject) => {
        const s = net.connect({ port, host: 'localhost' });
        s.on('connect', () => resolve(s));
        s.on('error', reject);
        setTimeout(() => { s.destroy(); reject(new Error('Connect timeout')); }, 1000);
      });
      break;
    } catch (e) {
      if (i === 19) throw e;
      await new Promise(r => setTimeout(r, 500));
    }
  }

  return new Promise((resolve, reject) => {
    let buffer = '';
    let addonsActorFound = false;
    let addonsActor = null;
    let installed = false;

    const onData = (data) => {
      const chunk = data.toString();
      buffer += chunk;
      
      if (!addonsActorFound && buffer.includes('addonsActor')) {
        const match = buffer.match(/"addonsActor":"([^"]+)"/);
        if (match) {
          addonsActorFound = true;
          addonsActor = match[1];
          console.log(`DEBUG: Found addonsActor: ${addonsActor}`);
          const installCmd = JSON.stringify({
            to: addonsActor,
            type: 'installTemporaryAddon',
            addonPath: addonAbsPath
          });
          socket.write(`${installCmd.length}:${installCmd}`);
          buffer = ''; 
        }
      } else if (!addonsActorFound && buffer.includes('"from":"root"')) {
        console.log(`DEBUG: Root response received, no addonsActor. Requesting listAddons...`);
        const listCmd = JSON.stringify({ to: 'root', type: 'listAddons' });
        socket.write(`${listCmd.length}:${listCmd}`);
        buffer = '';
      }

      if (buffer.includes('"addon"')) {
        // installTemporaryAddon's response carries only the manifest ID; the
        // internal UUID (moz-extension://<uuid>/) comes from listAddons.
        console.log('DEBUG: Addon installed, requesting listAddons...');
        installed = true;
        const listCmd = JSON.stringify({ to: 'root', type: 'listAddons' });
        socket.write(`${listCmd.length}:${listCmd}`);
        buffer = '';
      } else if (installed) {
        // Response to the listAddons request. Select our addon by its stable
        // manifest ID (the list also contains system addons) and take the
        // internal UUID from manifestURL.
        const msg = parseFrames(buffer);
        const addon = (msg && msg.addons || []).find(
          (a) => a && a.id === 'cosmarden@enikeev.com'
        );
        const addonId = addon && addon.manifestURL ? addon.manifestURL.split('/')[2] : null;
        if (!addonId) {
          console.error(`DEBUG: Could not find addon URL in listAddons response: ${buffer}`);
          cleanup();
          reject(new Error('no addon url in listAddons response'));
          return;
        }
        console.log(`DEBUG: Addon installed successfully, internal id: ${addonId}`);
        cleanup();
        resolve(addonId);
      }
      
      if (buffer.includes('"error"')) {
        console.error(`DEBUG: Error installing addon: ${buffer}`);
        cleanup();
        reject(new Error(`Firefox RD Error: ${buffer}`));
      }
    };

    const onError = (err) => {
      console.error(`DEBUG: Socket error: ${err.message}`);
      cleanup();
      reject(err);
    };

    const cleanup = () => {
      socket.removeListener('data', onData);
      socket.removeListener('error', onError);
      socket.end();
    };

    socket.on('data', onData);
    socket.on('error', onError);

    setTimeout(() => {
      cleanup();
      reject(new Error('Timeout waiting for addon installation response'));
    }, 30000);
    
    // Initial getRoot
    const getRootCmd = JSON.stringify({ to: 'root', type: 'getRoot' });
    socket.write(`${getRootCmd.length}:${getRootCmd}`);
  });
}

/**
 * Runs a Cosmarden CLI command and returns the output.
 */
export function runCli(args, env = {}) {
  const projectRoot = path.resolve(__dirname, '../../..');
  const cliPath = path.join(projectRoot, 'target/debug/cosmarden');
  
  // Intentionally inherits the XDG environment rather than redirecting it:
  // this CLI must resolve the *same* profile dirs as the agent that run-e2e.sh
  // started, and that agent runs against the real XDG roots under the
  // test-extension-e2e profile. Redirecting here would point the CLI at an
  // empty config. The dirs are removed by run-e2e.sh's cleanup() trap
  // (cleanup_profile) — see docs/test_cleanup_plan.md.
  const defaultEnv = {
    ...process.env,
    COSMARDEN_PROFILE: 'test-extension-e2e',
    ...env
  };

  try {
    return execSync(`"${cliPath}" ${args}`, { env: defaultEnv, encoding: 'utf8' });
  } catch (e) {
    console.error(`CLI Command Failed: ${args}`);
    console.error(e.stdout || e.message);
    throw e;
  }
}

/**
 * Gets the extension's internal ID (UUID) from Firefox.
 */
export async function getExtensionId(page) {
  await page.goto('about:debugging#/runtime/this-firefox');
  // This is tricky as about:debugging is a system page.
  // Alternatively, we can use the manifest ID "cosmarden@enikeev.com"
  // to find the internal UUID if we need to open the popup URL directly.
  return "cosmarden@enikeev.com";
}
