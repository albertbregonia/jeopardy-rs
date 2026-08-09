import type { PlayerCommand } from "../generated/PlayerCommand";
import type { LoginCredentials } from "./JoinLobby";

// derived from rust variant in backend
// defines what can be sent by the player over the websocket
// there is no equivalent to the rust enum, therefore,
// we simply have to make everything optional and check it
export interface PlayerRequest {
    login?: LoginCredentials
    command?: PlayerCommand
}

// derived from rust variant in backend
export interface PlayerResponse {
    result: PlayerCommandResponse | string
}

export interface PlayerCommandResponse { }

export class PlayerConn {
    readonly websocket: WebSocket;

    constructor(websocket: WebSocket) {
        this.websocket = websocket;
    }
}