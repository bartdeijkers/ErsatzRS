/* Runtime image loader probe: no GPU or build-tool dependencies in the final stage. */
#include <dlfcn.h>
#include <stdio.h>

int main(int argc, char **argv) {
    if (argc < 2) return 2;
    for (int i = 1; i < argc; ++i) {
        void *library = dlopen(argv[i], RTLD_NOW | RTLD_LOCAL);
        if (!library) {
            fprintf(stderr, "%s: %s\n", argv[i], dlerror());
            return 1;
        }
        printf("loaded %s\n", argv[i]);
        dlclose(library);
    }
    return 0;
}
