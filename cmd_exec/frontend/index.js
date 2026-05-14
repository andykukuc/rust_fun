// Dynamically determine the backend URL based on the current origin
const backendUrl = `${window.location.origin}`;

document.getElementById("executeButton").addEventListener("click", async () => {
    const commandInput = document.getElementById("commandInput").value.trim();
    const outputDiv = document.getElementById("output");

    if (commandInput === "") {
        outputDiv.textContent = "Please enter a command.";
        return;
    }

    try {
        const response = await fetch(`${backendUrl}/execute/${encodeURIComponent(commandInput)}`);
        const result = await response.json();

        if (result.success) {
            outputDiv.textContent = `Command Output:\n${result.message}`;
        } else {
            outputDiv.textContent = `Error:\n${result.message}`;
        }
    } catch (error) {
        outputDiv.textContent = `Failed to connect to the server: ${error.message}`;
    }
});

document.getElementById("currentDirButton").addEventListener("click", async () => {
    const currentDirOutput = document.getElementById("currentDirOutput");

    try {
        const response = await fetch(`${backendUrl}/current_dir`);
        const result = await response.json();

        if (result.success) {
            currentDirOutput.textContent = `Current Directory:\n${result.message}`;
        } else {
            currentDirOutput.textContent = `Error:\n${result.message}`;
        }
    } catch (error) {
        currentDirOutput.textContent = `Failed to connect to the server: ${error.message}`;
    }
});

document.getElementById("clearButton").addEventListener("click", () => {
    document.getElementById("output").textContent = "";
    document.getElementById("currentDirOutput").textContent = "";
});