let port = null;

function connect() {
    console.log("Connecting to cosmic-bwarden-agent...");
    port = browser.runtime.connectNative("com.8bit.cosmic_bwarden");
    
    port.onDisconnect.addListener((p) => {
        if (p.error) {
            console.error("Disconnected from agent with error:", p.error);
        } else {
            console.log("Disconnected from agent");
        }
        port = null;
    });
}

async function sendToAgent(action) {
    if (!port) {
        connect();
    }

    return new Promise((resolve, reject) => {
        const listener = (response) => {
            port.onMessage.removeListener(listener);
            resolve(response);
        };
        port.onMessage.addListener(listener);
        
        try {
            port.postMessage(action);
        } catch (e) {
            port.onMessage.removeListener(listener);
            reject(e);
        }
    });
}

browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
    // Forward messages from popup/content scripts to the agent
    return sendToAgent(message);
});
