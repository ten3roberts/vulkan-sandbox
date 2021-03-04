SHADERC=glslc

SHADERS=\
				default.vert.spv\
				default.frag.spv

all: shaders

shaders: $(SHADERS) 

# Compile shaders into SPIR-V
%.spv: ./data/shaders/%
	$(SHADERC) $^ -o ./data/shaders/$@

clean:
	rm ./data/shaders/*.spv
