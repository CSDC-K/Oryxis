import memory
db = memory.OryxisMemory('./mydb')
skills = db.list_skills()
result = []
for s in skills:
    result.append(f'Skill: {s["name"]}')
