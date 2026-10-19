CFLAGS ?= -std=c11 -Wall -Wextra -Werror -O2
VECTORS := ../../core/testdata/vectors.json
HID_RS := ../../core/src/hid.rs
DESC_H := ../openx3d/descriptor.h

.PHONY: test check-descriptor clean

test: test_pipeline check-descriptor
	./test_pipeline $(VECTORS)

test_pipeline: test_pipeline.c ../pipeline/pipeline.c ../pipeline/pipeline.h
	$(CC) $(CFLAGS) -I../pipeline -o $@ test_pipeline.c ../pipeline/pipeline.c

# The HID report descriptor exists twice (driver C, core Rust); they must be byte-identical.
check-descriptor:
	@if [ -f $(HID_RS) ]; then \
	  c=$$(sed -n '/G_ReportDescriptor\[\] = {/,/^};/p' $(DESC_H) | sed 's:/\*.*\*/::' | grep -o '0x[0-9A-Fa-f][0-9A-Fa-f]' | tr -d '\n' | tr 'a-f' 'A-F'); \
	  r=$$(sed -n '/REPORT_DESCRIPTOR: &\[u8\] = &\[/,/^\];/p' $(HID_RS) | sed 's://.*::' | grep -o '0x[0-9A-Fa-f][0-9A-Fa-f]' | tr -d '\n' | tr 'a-f' 'A-F'); \
	  if [ -n "$$c" ] && [ "$$c" = "$$r" ]; then echo "descriptor: driver and core copies match"; \
	  else echo "descriptor: $(DESC_H) differs from $(HID_RS)"; exit 1; fi; \
	else echo "descriptor: $(HID_RS) not found, skipped"; fi

clean:
	rm -f test_pipeline test_pipeline.exe
