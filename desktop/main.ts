import { app, BrowserWindow } from "electron";
import path from "node:path";

const devUrl = process.env.ECHO_UI_DEV_URL;
const openDevTools = process.env.ECHO_OPEN_DEVTOOLS === "1";

function createWindow() {
  const window = new BrowserWindow({
    width: 1120,
    height: 760,
    minWidth: 900,
    minHeight: 620,
    backgroundColor: "#020202",
    titleBarStyle: "hiddenInset",
    trafficLightPosition: { x: 18, y: 18 },
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  if (devUrl) {
    window.loadURL(devUrl).catch(console.error);
    if (openDevTools) {
      window.webContents.openDevTools({ mode: "detach" });
    }
  } else {
    window
      .loadFile(path.join(__dirname, "..", "..", "frontend", "dist", "index.html"))
      .catch(console.error);
  }
}

app.whenReady().then(() => {
  createWindow();
  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    app.quit();
  }
});
