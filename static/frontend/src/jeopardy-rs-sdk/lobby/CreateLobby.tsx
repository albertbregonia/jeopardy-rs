import type { JeopardyConfig } from "../game/Jeopardy";

export interface CreateLobbyRequest {
    lobbyName: string,
    lobbyPassword: string,
    hostPassword: string,
    config: JeopardyConfig,
}

export interface CreateLobbyResponse {
    requestId: string,
    error: string | null
}

const CREATE_LOBBY_PATH = "/lobbies"

export async function createLobby(request: CreateLobbyRequest): Promise<CreateLobbyResponse> {
    const rawResponse = await fetch(CREATE_LOBBY_PATH, {
        method: 'POST',
        headers: { "Content-Type": 'application/json' },
        body: JSON.stringify(request)
    });
    // TODO: error handling
    return await rawResponse.json();
}