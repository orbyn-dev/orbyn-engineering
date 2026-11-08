import base from "./playwright.config";
export default { ...base, webServer: { ...base.webServer!, command: "npx vite --port 5206 --strictPort", url: "http://localhost:5206" }, use: { ...base.use, baseURL: "http://localhost:5206" } };
