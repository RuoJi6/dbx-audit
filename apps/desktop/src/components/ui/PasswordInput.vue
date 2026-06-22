<script setup lang="ts">
import { computed, ref } from "vue";
import { Eye, EyeOff } from "@lucide/vue";
import { Input } from "@/components/ui/input";

const model = defineModel<string>();

const props = defineProps<{
  placeholder?: string;
  disabled?: boolean;
  class?: string;
  inputClass?: string;
  showToggle?: boolean;
}>();

const visible = ref(false);
const showToggle = computed(() => props.showToggle ?? true);
</script>

<template>
  <div :class="props.class" class="relative">
    <Input v-model="model" :type="visible ? 'text' : 'password'" :placeholder="placeholder" :disabled="disabled" :class="[props.inputClass, showToggle ? 'pr-8' : undefined]" v-bind="$attrs" />
    <button v-if="showToggle" type="button" :disabled="disabled" class="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground disabled:pointer-events-none" tabindex="-1" @click="visible = !visible">
      <Eye v-if="!visible" class="size-3.5" />
      <EyeOff v-else class="size-3.5" />
    </button>
  </div>
</template>

<style scoped>
:deep(input[type="password"]::-ms-reveal),
:deep(input[type="password"]::-ms-clear) {
  display: none;
}

:deep(input[type="password"]::-webkit-credentials-auto-fill-button),
:deep(input[type="password"]::-webkit-caps-lock-indicator),
:deep(input[type="password"]::-webkit-contacts-auto-fill-button) {
  display: none !important;
  visibility: hidden;
  pointer-events: none;
}
</style>
