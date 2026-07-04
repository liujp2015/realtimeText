import { createApp } from "vue";
import { createPinia } from "pinia";
import { createRouter, createWebHashHistory } from "vue-router";
import App from "./App.vue";
import Settings from "./views/Settings.vue";
import History from "./views/History.vue";

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", redirect: "/settings" },
    { path: "/settings", component: Settings },
    { path: "/history", component: History },
  ],
});

createApp(App).use(createPinia()).use(router).mount("#app");
