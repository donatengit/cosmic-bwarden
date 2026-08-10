import net from 'net';
import path from 'path';
import { execSync } from 'child_process';

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
    
    const onData = (data) => {
      const chunk = data.toString();
      buffer += chunk;
      
      if (!addonsActorFound && buffer.includes('addonsActor')) {
        const match = buffer.match(/"addonsActor":"([^"]+)"/);
        if (match) {
          addonsActorFound = true;
          const actor = match[1];
          console.log(`DEBUG: Found addonsActor: ${actor}`);
          const installCmd = JSON.stringify({
            to: actor,
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
        console.log('DEBUG: Addon installed successfully');
        cleanup();
        resolve(true);
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
 * Runs a COSMIC BWarden CLI command and returns the output.
 */
export function runCli(args, env = {}) {
  const projectRoot = path.resolve(__dirname, '../../..');
  const cliPath = path.join(projectRoot, 'target/debug/cosmic-bwarden-cli');
  
  const defaultEnv = {
    ...process.env,
    COSMIC_BWARDEN_PROFILE: 'test-extension-e2e',
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
  // Alternatively, we can use the manifest ID "cosmic-bwarden@enikeev.com"
  // to find the internal UUID if we need to open the popup URL directly.
  return "cosmic-bwarden@enikeev.com";
}
