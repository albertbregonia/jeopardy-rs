// derived from the rust models in the backend

export interface Question {
    content: string,
    answer: string
}

export interface BoardQuestion {
    pointValue: number,
    dailyDouble: boolean,
    answered: boolean,
    question: Question,
}

export interface Category {
    name: string,
    questions: BoardQuestion[]
}

export interface Board {
    categories: Category[]
}

export interface FinalJeopardy {
    hint: string,
    question: Question,
}

export interface JeopardyConfig {
    boards: Board[],
    final_jeopardy: FinalJeopardy,
}

export interface JeopardyPlayer {
    points: number,
    name: string,
    buzzed: boolean,
}