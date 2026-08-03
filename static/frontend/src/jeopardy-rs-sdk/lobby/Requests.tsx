// typescript variants of the types defined to interface with the backend

import type { JeopardyConfig } from "../game/Jeopardy";

export interface CreateLobbyRequest {
    lobbyName: string,
    lobbyPassword: string,
    hostPassword: string,
    config: JeopardyConfig,
}