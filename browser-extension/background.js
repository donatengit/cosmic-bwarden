if (typeof globalThis.browser === 'undefined') { globalThis.browser = globalThis.chrome; }

let port = null;
const queue = [];
let isProcessing = false;

function connect() {
    console.log("Connecting to cosmic-bwarden-agent...");
    port = browser.runtime.connectNative("com.8bit.cosmic_bwarden");
    
    port.onDisconnect.addListener((p) => {
        const lastErr = chrome.runtime.lastError;
        if (p.error || lastErr) {
            console.error("Disconnected from agent:", p.error || (lastErr && lastErr.message));
        } else {
            console.log("Disconnected from agent");
        }
        port = null;
        // Reject all pending in queue
        while (queue.length > 0) {
            const req = queue.shift();
            req.reject(new Error("Agent disconnected: " + (p.error || "unknown error")));
        }
        isProcessing = false;
    });

    port.onMessage.addListener((response) => {
        const req = queue.shift();
        if (req) {
            req.resolve(response);
        }
        isProcessing = false;
        processQueue();
    });
}

function processQueue() {
    if (isProcessing || queue.length === 0) return;
    
    if (!port) {
        connect();
    }

    isProcessing = true;
    const { action, reject } = queue[0]; 
    // We don't shift until we get a response (or error)
    
    try {
        port.postMessage(action);
    } catch (e) {
        console.error("Failed to post message:", e);
        queue.shift();
        isProcessing = false;
        reject(e);
        processQueue();
    }
}

async function sendToAgent(action) {
    return new Promise((resolve, reject) => {
        queue.push({ action, resolve, reject });
        processQueue();
    });
}

browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
    // Forward messages from popup/content scripts to the agent
    return sendToAgent(message);
});
