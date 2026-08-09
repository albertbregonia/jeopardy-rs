import { PlayerConn, type PlayerRequest } from "./PlayerConn";

export interface LoginCredentials {
    lobbyId: string,
    lobbyPassword: string,
    username: string,
}

const JOIN_LOBBY_PATH = "/lobbies"

// TODO: this function does not utilize the backend's retry system
// and does not re-use the websocket connection / propagate the errors back
export async function joinLobby(request: LoginCredentials): Promise<PlayerConn> {
    const websocket = new WebSocket(JOIN_LOBBY_PATH);
    return new Promise((resolve, reject) => {
        websocket.onclose = (e) => {
            reject(new Error(e.reason));
        };
        websocket.onopen = () => {
            const loginRequest: PlayerRequest = {
                login: request
            }
            websocket.send(JSON.stringify(loginRequest));
            // backend expects this to be sent as the first msg
            // but will not reply if successful. it will only reply
            // with an error message and disconnect if one occurs
            // TODO: i may change that
            resolve(new PlayerConn(websocket));
        };
    });
}