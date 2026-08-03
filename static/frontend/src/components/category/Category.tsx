import type { Category } from "../../jeopardy-rs-sdk/game/Jeopardy"
import { Cell } from "../cell/Cell"
import "./Category.css"

export interface CategoryProps {
    category: Category
}

// Categories are a simple <div> that contain a collection of <Cells> 
// that scale to the size of the available space.
export function Category({ category }: CategoryProps) {
    return (
        <div className="category">
            <Cell className="category-name-cell">
                <div className="category-name">{category.name}</div>
            </Cell>
            <div className="category-questions">{
                category.questions
                    .map((q, index) => // keys need to be unique
                        <Cell key={`${category.name}-${index}`}
                            className="category-question">
                            <div className="points">${q.answered ? "" : q.pointValue.toString()}</div>
                        </Cell>
                    )
            }</div>
        </div>
    )
}