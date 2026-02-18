import { invoke } from "@tauri-apps/api/core";
import "./style.css";

const app = document.querySelector<HTMLDivElement>("#app")!;

app.innerHTML = `
  <div class="min-h-screen bg-gradient-to-br from-purple-500 via-pink-500 to-red-500 flex items-center justify-center p-8">
    <div class="bg-white rounded-2xl shadow-2xl p-8 max-w-md w-full">
      <div class="text-center mb-8">
        <h1 class="text-4xl font-bold text-gray-800 mb-2">Deadcat.Live</h1>
        <p class="text-gray-600">A Tauri App with Tailwind CSS</p>
      </div>

      <div class="space-y-4">
        <div class="bg-gradient-to-r from-purple-100 to-pink-100 rounded-lg p-4">
          <h2 class="text-xl font-semibold text-gray-800 mb-2">Welcome!</h2>
          <p class="text-gray-600">This is a demo Tauri application with Tailwind CSS styling.</p>
        </div>

        <div class="bg-gray-50 rounded-lg p-4">
          <label for="name-input" class="block text-sm font-medium text-gray-700 mb-2">
            Enter your name:
          </label>
          <input
            id="name-input"
            type="text"
            placeholder="Your name"
            class="w-full px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-purple-500 focus:border-transparent outline-none transition"
          />
        </div>

        <button
          id="greet-button"
          class="w-full bg-gradient-to-r from-purple-500 to-pink-500 hover:from-purple-600 hover:to-pink-600 text-white font-semibold py-3 px-6 rounded-lg shadow-lg transform transition hover:scale-105 active:scale-95"
        >
          Greet
        </button>

        <div id="greet-msg" class="hidden bg-green-50 border border-green-200 rounded-lg p-4 text-green-800"></div>
      </div>

      <div class="mt-8 pt-6 border-t border-gray-200">
        <div class="flex justify-center space-x-4">
          <span class="inline-flex items-center px-3 py-1 rounded-full text-xs font-medium bg-purple-100 text-purple-800">
            Tauri
          </span>
          <span class="inline-flex items-center px-3 py-1 rounded-full text-xs font-medium bg-pink-100 text-pink-800">
            Tailwind CSS
          </span>
          <span class="inline-flex items-center px-3 py-1 rounded-full text-xs font-medium bg-blue-100 text-blue-800">
            TypeScript
          </span>
        </div>
      </div>
    </div>
  </div>
`;

const nameInput = document.querySelector<HTMLInputElement>("#name-input")!;
const greetButton = document.querySelector<HTMLButtonElement>("#greet-button")!;
const greetMsg = document.querySelector<HTMLDivElement>("#greet-msg")!;

async function greet() {
  const name = nameInput.value || "World";
  const greeting = await invoke<string>("greet", { name });
  greetMsg.textContent = greeting;
  greetMsg.classList.remove("hidden");
}

greetButton.addEventListener("click", greet);
nameInput.addEventListener("keypress", (e) => {
  if (e.key === "Enter") {
    greet();
  }
});
