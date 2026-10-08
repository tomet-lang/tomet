#!/usr/bin/env -S deno run --allow-run

const command = new Deno.Command("git", {
  args: ["ls-files", "--others", "--ignored", "--exclude-standard", ".tomet"],
  stdout: "piped",
  stderr: "piped",
});

const output = await command.output();
if (!output.success) {
  console.error(new TextDecoder().decode(output.stderr));
  Deno.exit(1);
}

const stdout = new TextDecoder().decode(output.stdout).trim();
if (stdout.length > 0) {
  const files = stdout.split(/\r?\n/);
  for (const file of files) {
    console.error(
      `AUTHORED FAIL: ${file} -- ignored file under .tomet; dot-tomet-is-authored says every file there is hand-written`,
    );
  }
  Deno.exit(1);
}
