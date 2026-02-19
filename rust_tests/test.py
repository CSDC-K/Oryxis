import memory

desc = """
TAGS: cmd, terminal, shell, command line, system commands, open, application launcher, file management, process management, application, app
FAST_EXECUTE : ACTIVE
SKILL: cmdlib
DESCRIPTION: Executes Windows shell commands and launches applications.

[FAST_EXECUTE]

cmdlib.run_command("cmd", ["/C", "start", "spotify://"])

IMPORTANT: dont use "" because it can be destroy json format, use '' instead.

"""


def task():
    db = memory.OryxisMemory('./mydb')
    db.edit_skill("cmd", desc, "SELF SKILL.")

test = task()
print(test)