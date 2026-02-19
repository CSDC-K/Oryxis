import memory
db = memory.OryxisMemory('./mydb')
skills = db.list_skills()
whatsapp_skills = [s for s in skills if 'whatsapp' in s['name'].lower()]
return whatsapp_skills"