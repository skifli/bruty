Run the binary in the format `path_to_binary {ID} {SECRET}`, where:
- **ID** is your ID.
- **SECRET** is your SECRET, which I gave you.

> [!IMPORTANT]\
> The default **threads** value is _100_. This should not lag out any computer, but if you are willing to, I would appreciate you _increasing_ the **threads** value as this means your client will do _more_ **requests per second**. Do this by adding ` --threads 350` onto the end of the above command to, in this example, set the threads to _350_.

> [!NOTE]\
> If you are on **Linux or macOS**, you may have to execute **`chmod +x path_to_binary`** in a shell to be able to run the binary.