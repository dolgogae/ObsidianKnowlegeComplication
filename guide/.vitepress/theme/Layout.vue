<script setup lang="ts">
import DefaultTheme from "vitepress/theme";
import { onBeforeUnmount, onMounted, ref } from "vue";

const progress = ref(0);
const { Layout } = DefaultTheme;

function updateProgress() {
  const root = document.documentElement;
  const distance = root.scrollHeight - root.clientHeight;
  progress.value = distance > 0 ? Math.min(1, root.scrollTop / distance) : 0;
}

onMounted(() => {
  updateProgress();
  window.addEventListener("scroll", updateProgress, { passive: true });
  window.addEventListener("resize", updateProgress);
});

onBeforeUnmount(() => {
  window.removeEventListener("scroll", updateProgress);
  window.removeEventListener("resize", updateProgress);
});
</script>

<template>
  <Layout>
    <template #layout-top>
      <div
        class="okc-reading-progress"
        :style="{ transform: `scaleX(${progress})` }"
        aria-hidden="true"
      />
    </template>
  </Layout>
</template>
