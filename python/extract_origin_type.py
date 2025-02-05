import os
from pyarrow import orc
import pickle
import time

start_time = time.time()

prefix_origin_visit = "/home/infres/rapaport/datasets/2024-08-23/origin_visit/"
def build_db_visit_type(prefix):
    orc_file_list = os.listdir(prefix)
    database = {}
    for orc_file in orc_file_list:
        table = orc.read_table(prefix + orc_file)
        for i in range(len(table[0])):
            current_origin = table[0][i].as_py()
            if current_origin not in database:
                current_visit_type = table[3][i].as_py()
                database[current_origin] = current_visit_type
    return database
db_visit = build_db_visit_type(prefix_origin_visit)

print("Extraction --- %s seconds ---" % (time.time() - start_time))

start_time = time.time()
with open('db.pkl', 'wb') as f:
    pickle.dump(db_visit, f)
    
print("Saving --- %s seconds ---" % (time.time() - start_time))
########## 2 hours