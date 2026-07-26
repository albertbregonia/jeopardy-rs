import type { Category as CategoryModel } from "./types/Jeopardy";
import { Board } from "./components/board/Board";
import './App.css'

function App() {
    // dummy data to fill the components
    const category: CategoryModel = {
        name: "Category Name",
        questions: Array.from({ length: 5 }, () => ({
            answered: false,
            dailyDouble: false,
            pointValue: Math.round(Math.random() * 1000 + 100),
            question: {
                content: "Question",
                answer: "Answer"
            }
        }))
    };
    const longNamedCategory: CategoryModel = {
        name: "Lorem ipsum dolor sit amet consectetur, adipisicing elit.Eos esse doloremque eum corrupti nam quibusdam perspiciatis iste veniam molestias, aut sed pariatur numquam totam, vel similique quod mollitia a.Velit!",
        questions: Array.from({ length: 1 }, () => ({
            answered: false,
            dailyDouble: false,
            pointValue: Math.round(Math.random() * 1000 + 100),
            question: {
                content: "Question",
                answer: "Answer"
            }
        }))
    };
    return (
        <>
            <Board board={{
                categories: [
                    longNamedCategory,
                    category,
                    category,
                    category,
                    category,
                    category,
                    category,
                    category,
                    category,
                    category
                ]
            }} />
        </>
    )
}

export default App
