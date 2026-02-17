import { WebSocketConnection } from "./utils/ws";

async function main() {
	let ws = await WebSocketConnection.connect("/api");
	while (ws.is_ok()) {
		console.log(await ws.recv());
	}
}

main();
