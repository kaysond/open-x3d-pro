CFLAGS ?= -std=c11 -Wall -Wextra -Werror -O2
VECTORS := ../../core/testdata/vectors.json

.PHONY: test clean

test: test_pipeline
	./test_pipeline $(VECTORS)

test_pipeline: test_pipeline.c ../pipeline/pipeline.c ../pipeline/pipeline.h
	$(CC) $(CFLAGS) -I../pipeline -o $@ test_pipeline.c ../pipeline/pipeline.c

clean:
	rm -f test_pipeline test_pipeline.exe
