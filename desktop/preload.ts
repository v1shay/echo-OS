import { contextBridge } from "electron";

contextBridge.exposeInMainWorld("echoDesktop", {
  backendHttpUrl: process.env.ECHO_BACKEND_HTTP_URL ?? "http://127.0.0.1:8000",
  backendWsUrl: process.env.ECHO_BACKEND_WS_URL ?? "ws://127.0.0.1:8000/ws/voice/default",
});
